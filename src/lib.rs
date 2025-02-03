use std::any::TypeId;

use bevy::{prelude::*, reflect::Reflectable};
use ego_tree::*;

pub mod dyn_bundle;
mod plugin;

use dyn_bundle::prelude::*;
pub use ego_tree;
use plugin::*;

pub mod prelude {
    pub use super::plugin::{BehaveCtx, BehavePlugin, BehaveSet, BehaveTargetEntity, BehaveTree};
    pub use super::{Behave, BehaveFinished};
    // the ego_tree `tree!` macro
    pub use ego_tree::tree;
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BehaveNodeStatus {
    Success,
    Failure,
    Running,
    AwaitingTrigger,
}

/// If present, don't tick tree.
/// means tree is sleeping, until a trigger reports a status (which removes the component)
#[derive(Component)]
struct BehaveAwaitingTrigger;

/// Inserted on the entity with the BehaveTree when the tree has finished executing.
/// Containts the final result of the tree.
#[derive(Component, Reflect, Debug)]
pub struct BehaveFinished(pub bool);

/// A behaviour added to the tree by a user, which we convert to a a BehaviourNode tree internally
/// to run the tree. This is the template of the behaviour without all the internal runtime state.
///
/// Constuction is via static fns on Behave, so we can do the dynamic bundle stuff.
/// although this probably makes it hard to load the tree def from an asset file?
///
/// TODO: could have an existing entity task too, that we insert a Running component on to start it.
// #[derive(Debug)]
pub trait BehaveConditionTrigger: Send + Sync {
    fn trigger(&self, commands: &mut Commands, ctx: BehaveTriggerCtx);
}

impl<T: Clone + Send + Sync + 'static> BehaveConditionTrigger for T {
    fn trigger(&self, commands: &mut Commands, ctx: BehaveTriggerCtx) {
        commands.trigger(BehaveCondition::<T> {
            value: self.clone(),
            ctx,
        });
    }
}

pub enum Behave {
    /// Waits this many seconds before Succeeding
    Wait(f32),
    /// Spawns an entity, and waits for it to trigger a status report
    /// Use the Behaviour::spawn_entity constructor, or import dyn_bundle (not in prelude)
    DynamicEntity(DynamicBundel),
    /// Runs children in sequence, failing if any fails, succeeding if all succeed
    Sequence,
    /// Runs children in sequence until one succeeds. If all fail, this fails.
    Fallback,
    /// Inverts success/failure of child. Must only have one child.
    Invert,
    /// Always succeeds
    AlwaysSucceed,
    /// Always fails
    AlwaysFail,
    /// Returns a result from a trigger
    Conditional(Box<dyn BehaveConditionTrigger>),
    /// Loops forever
    Forever,
}

impl Behave {
    pub fn dynamic_spawn<T: Bundle + Clone>(bundle: T) -> Behave {
        Behave::DynamicEntity(DynamicBundel::new(bundle))
    }
    pub fn conditional<T: Clone + Send + Sync + 'static>(value: T) -> Self {
        Behave::Conditional(Box::new(value))
    }
}

/// A wrapper around a user-provided type, which we trigger to test a condition.
#[derive(Event, Debug, Clone)]
pub struct BehaveCondition<T: Clone + Send + Sync> {
    pub(crate) value: T,
    pub(crate) ctx: BehaveTriggerCtx,
}

impl<T: Clone + Send + Sync> BehaveCondition<T> {
    pub fn ctx(&self) -> &BehaveTriggerCtx {
        &self.ctx
    }
    pub fn value(&self) -> &T {
        &self.value
    }
}

// pub struct BehaveConditional<const ID: &'static str>(Box<dyn Reflect>);

// pub trait BehaveCondition {
//     fn id() -> &'static str;
// }

// #[derive(Reflect)]
// pub struct DistCond(f32);
// impl BehaveCondition for DistCond {
//     fn id() -> &'static str {
//         "DistCond"
//     }
// }

/// A state wraps the behaviour, and is the node in our internal tree representation of the behaviour tree
/// One per Behave, with extra state bits.
// #[derive(Debug)]
pub(crate) enum BehaveNode {
    Forever {
        status: Option<BehaveNodeStatus>,
    },
    Wait {
        start_time: Option<f32>,
        secs_to_wait: f32,
        status: Option<BehaveNodeStatus>,
    },
    DynamicEntity {
        // None until something spawned.
        task_status: EntityTaskStatus,
        status: Option<BehaveNodeStatus>,
        bundle: DynamicBundel,
    },
    SequenceFlow {
        status: Option<BehaveNodeStatus>,
    },
    FallbackFlow {
        status: Option<BehaveNodeStatus>,
    },
    Invert {
        status: Option<BehaveNodeStatus>,
    },
    AlwaysSucceed {
        status: Option<BehaveNodeStatus>,
    },
    AlwaysFail {
        status: Option<BehaveNodeStatus>,
    },
    Conditional {
        status: Option<BehaveNodeStatus>,
        task_status: TriggerTaskStatus,
        trigger: Box<dyn BehaveConditionTrigger>,
    },
}

#[derive(Clone, Debug)]
enum EntityTaskStatus {
    NotStarted,
    Started(Entity),
    Complete(bool),
}

#[derive(Clone, Debug)]
enum TriggerTaskStatus {
    NotTriggered,
    Triggered,
    Complete(bool),
}

impl BehaveNode {
    fn existing_status(&self) -> &Option<BehaveNodeStatus> {
        match self {
            BehaveNode::Forever { status } => status,
            BehaveNode::Conditional { status, .. } => status,
            BehaveNode::Wait { status, .. } => status,
            BehaveNode::DynamicEntity { status, .. } => status,
            BehaveNode::SequenceFlow { status } => status,
            BehaveNode::FallbackFlow { status } => status,
            BehaveNode::Invert { status } => status,
            BehaveNode::AlwaysSucceed { status } => status,
            BehaveNode::AlwaysFail { status } => status,
        }
    }
}

impl std::fmt::Display for BehaveNode {
    #[rustfmt::skip]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BehaveNode::Forever { .. } => write!(f, "Forever")?,
            BehaveNode::Conditional { .. } => write!(f, "Conditional")?,
            BehaveNode::Wait { secs_to_wait, .. } => write!(f, "Wait({secs_to_wait})")?,
            BehaveNode::DynamicEntity { task_status, .. } => write!(f, "SpawnTask({task_status:?})")?,
            BehaveNode::SequenceFlow { .. } => write!(f, "SequenceFlow")?,
            BehaveNode::FallbackFlow { .. } => write!(f, "FallbackFlow")?,
            BehaveNode::Invert { .. } => write!(f, "Invert")?,
            BehaveNode::AlwaysSucceed { .. } => write!(f, "AlwaysSucceed")?,
            BehaveNode::AlwaysFail { .. } => write!(f, "AlwaysFail")?,
        }
        match self.existing_status() {
            Some(BehaveNodeStatus::Success) => write!(f, " --> ✅"),
            Some(BehaveNodeStatus::Failure) => write!(f, " --> ❌"),
            Some(BehaveNodeStatus::Running) => write!(f, " --> ⏳"),
            Some(BehaveNodeStatus::AwaitingTrigger) => write!(f, " --> ⏳"),
            _ => Ok(()),
        }
    }
}

impl BehaveNode {
    pub(crate) fn reset(&mut self) {
        match self {
            BehaveNode::Forever { status } => {
                *status = None;
            }
            BehaveNode::Conditional {
                status,
                task_status,
                ..
            } => {
                *status = None;
                *task_status = TriggerTaskStatus::NotTriggered;
            }
            BehaveNode::Wait {
                status, start_time, ..
            } => {
                *status = None;
                *start_time = None;
            }
            BehaveNode::DynamicEntity {
                status,
                task_status,
                ..
            } => {
                *status = None;
                *task_status = EntityTaskStatus::NotStarted;
            }
            BehaveNode::SequenceFlow { status } => {
                *status = None;
            }
            BehaveNode::FallbackFlow { status } => {
                *status = None;
            }
            BehaveNode::Invert { status } => {
                *status = None;
            }
            BehaveNode::AlwaysSucceed { status } => {
                *status = None;
            }
            BehaveNode::AlwaysFail { status } => {
                *status = None;
            }
        }
    }
    pub(crate) fn new(behave: Behave) -> Self {
        match behave {
            Behave::Forever => Self::Forever { status: None },
            Behave::Conditional(trig_fn) => Self::Conditional {
                status: None,
                task_status: TriggerTaskStatus::NotTriggered,
                trigger: trig_fn,
            },
            Behave::Wait(secs_to_wait) => Self::Wait {
                start_time: None,
                secs_to_wait,
                status: None,
            },
            Behave::DynamicEntity(bundle) => Self::DynamicEntity {
                task_status: EntityTaskStatus::NotStarted,
                status: None,
                bundle,
            },
            Behave::Sequence => Self::SequenceFlow { status: None },
            Behave::Fallback => Self::FallbackFlow { status: None },
            Behave::Invert => Self::Invert { status: None },
            Behave::AlwaysSucceed => Self::AlwaysSucceed { status: None },
            Behave::AlwaysFail => Self::AlwaysFail { status: None },
        }
    }
}

fn reset_descendants(n: &mut NodeMut<BehaveNode>) {
    // info!("Restting node: {:?}", n.id());
    n.value().reset();
    if let Some(mut sibling) = n.next_sibling() {
        reset_descendants(&mut sibling);
    }
    if let Some(mut child) = n.first_child() {
        reset_descendants(&mut child);
    }
}

fn tick_node(
    n: &mut NodeMut<BehaveNode>,
    time: &Res<Time>,
    ecmd: &mut EntityCommands<'_>,
    target_entity: Entity,
) -> BehaveNodeStatus {
    let bt_entity = ecmd.id();
    use BehaveNode::*;
    debug!("tick_node: {:?} = {}", n.id(), n.value());
    // short circuit nodes that have already got a result, or are blocked waiting for a trigger
    match n.value().existing_status() {
        None => {}
        Some(BehaveNodeStatus::Running) => {}
        Some(BehaveNodeStatus::AwaitingTrigger) => {}
        Some(BehaveNodeStatus::Success) => return BehaveNodeStatus::Success,
        Some(BehaveNodeStatus::Failure) => return BehaveNodeStatus::Failure,
    }
    let task_node = n.id();
    match n.value() {
        Forever { .. } => {
            let Forever { status } = n.value() else {
                unreachable!("Must be a Forever");
            };
            *status = Some(BehaveNodeStatus::Running);
            let mut only_child = n.first_child().expect("Forever nodes must have a child");
            if only_child.has_siblings() {
                panic!("Forever nodes must have a single child, not multiple children");
            }
            match tick_node(&mut only_child, time, ecmd, target_entity) {
                BehaveNodeStatus::Success | BehaveNodeStatus::Failure => {
                    // reset so we can run it again next tick
                    reset_descendants(&mut only_child);
                    BehaveNodeStatus::Running
                }
                other => other,
            }
        }
        Conditional {
            task_status: task_status @ TriggerTaskStatus::NotTriggered,
            status,
            trigger,
        } => {
            trigger.trigger(
                &mut ecmd.commands(),
                BehaveTriggerCtx {
                    bt_entity,
                    task_node,
                    target_entity,
                },
            );
            // Don't use AwaitingTrigger for this, because of ordering issues..
            // the trigger response arrives BEFORE we insert the BehaveAwaitingTrigger component,
            // so the trigger response handler can't remove it, so it never ticks.
            *task_status = TriggerTaskStatus::Triggered;
            *status = Some(BehaveNodeStatus::Running);
            BehaveNodeStatus::Running
        }
        #[rustfmt::skip]
        Conditional {task_status: TriggerTaskStatus::Complete(true), status, ..} => {
            *status = Some(BehaveNodeStatus::Success);
            BehaveNodeStatus::Success
        }
        #[rustfmt::skip]
        Conditional {task_status: TriggerTaskStatus::Complete(false), status, ..} => {
            *status = Some(BehaveNodeStatus::Failure);
            BehaveNodeStatus::Failure
        }
        Conditional {
            task_status: TriggerTaskStatus::Triggered,
            ..
        } => unreachable!("Should have short circuited while awiaint trigger"),
        Invert { .. } => {
            let mut only_child = n.first_child().expect("Invert nodes must have a child");
            if only_child.has_siblings() {
                panic!("Invert nodes must have a single child, not multiple children");
            }
            let res = match tick_node(&mut only_child, time, ecmd, target_entity) {
                BehaveNodeStatus::Success => BehaveNodeStatus::Failure, // swapped
                BehaveNodeStatus::Failure => BehaveNodeStatus::Success, // swapped
                BehaveNodeStatus::Running => BehaveNodeStatus::Running,
                BehaveNodeStatus::AwaitingTrigger => BehaveNodeStatus::AwaitingTrigger,
            };
            let Invert { status } = n.value() else {
                unreachable!("Must be an Invert");
            };
            *status = Some(res);
            res
        }
        AlwaysSucceed { status } => {
            *status = Some(BehaveNodeStatus::Success);
            BehaveNodeStatus::Success
        }
        AlwaysFail { status } => {
            *status = Some(BehaveNodeStatus::Failure);
            BehaveNodeStatus::Failure
        }
        // start waiting
        Wait {
            start_time: start_time @ None,
            status,
            ..
        } => {
            info!("Starting wait");
            *start_time = Some(time.elapsed_secs());
            *status = Some(BehaveNodeStatus::Running);
            BehaveNodeStatus::Running
        }
        // continue waiting
        Wait {
            start_time: Some(start_time),
            secs_to_wait,
            status,
        } => {
            // info!("Waiting");
            let elapsed = time.elapsed_secs() - *start_time;
            if elapsed > *secs_to_wait {
                *status = Some(BehaveNodeStatus::Success);
                return BehaveNodeStatus::Success;
            }
            BehaveNodeStatus::Running
        }
        // spawn a new entity for this task
        DynamicEntity {
            task_status: task_status @ EntityTaskStatus::NotStarted,
            status,
            bundle,
        } => {
            let id = ecmd.commands().spawn(()).id();
            let ctx = BehaveCtx {
                bt_entity,
                task_entity: id,
                target_entity,
            };
            ecmd.commands()
                .entity(id)
                .insert(ctx)
                .dyn_insert(bundle.clone());
            info!("Spawn task, Spawning entity: {id:?}");
            ecmd.add_child(id);
            *task_status = EntityTaskStatus::Started(id);
            *status = Some(BehaveNodeStatus::AwaitingTrigger);
            BehaveNodeStatus::AwaitingTrigger
        }
        #[rustfmt::skip]
        DynamicEntity{ task_status: EntityTaskStatus::Started(_), .. } => unreachable!("Short circuit should prevent this while AwaitingTrigger"),
        // this is when a trigger has reported a result, and we need to process it and update status
        #[rustfmt::skip]
        DynamicEntity {task_status: EntityTaskStatus::Complete(true), status, ..} => {
            *status = Some(BehaveNodeStatus::Success);
            BehaveNodeStatus::Success
        }
        #[rustfmt::skip]
        DynamicEntity {task_status: EntityTaskStatus::Complete(false), status, ..} => {
            *status = Some(BehaveNodeStatus::Failure);
            BehaveNodeStatus::Failure
        }
        // don't bind any fields here because we need to mutably borrow the node again
        SequenceFlow { .. } => {
            // info!("SequenceFlow. Processing children");
            let Some(mut child) = n.first_child() else {
                warn!("SequenceFlow with no children, returning success anyway");
                return BehaveNodeStatus::Success;
            };

            let mut final_status;
            loop {
                match tick_node(&mut child, time, ecmd, target_entity) {
                    BehaveNodeStatus::Success => {
                        final_status = BehaveNodeStatus::Success;
                        if let Ok(next_child) = child.into_next_sibling() {
                            child = next_child;
                            continue;
                        } else {
                            break;
                        }
                    }
                    // A non-success state just gets bubbled up to the parent
                    other => {
                        final_status = other;
                        break;
                    }
                }
            }
            let SequenceFlow { status, .. } = n.value() else {
                unreachable!("Must be a SequenceFlow");
            };
            *status = Some(final_status);
            final_status
        }

        FallbackFlow { .. } => {
            let Some(mut child) = n.first_child() else {
                warn!("FallbackFlow with no children, returning success anyway");
                return BehaveNodeStatus::Success;
            };

            let mut final_status;
            loop {
                match tick_node(&mut child, time, ecmd, target_entity) {
                    BehaveNodeStatus::Failure => {
                        // a child fails, try the next one, or if no more children, we failed.
                        final_status = BehaveNodeStatus::Failure;
                        if let Ok(next_child) = child.into_next_sibling() {
                            child = next_child;
                            continue;
                        } else {
                            break;
                        }
                    }
                    // A non-failure state just gets bubbled up to the parent
                    other => {
                        final_status = other;
                        break;
                    }
                }
            }
            let FallbackFlow { status, .. } = n.value() else {
                unreachable!("Must be a FallbackFlow");
            };
            *status = Some(final_status);
            final_status
        }
    }
}
