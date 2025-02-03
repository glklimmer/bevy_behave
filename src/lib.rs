use bevy::prelude::*;
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

pub type BoxedConditionSystem = Box<dyn System<In = In<BehaveCtx>, Out = bool>>;

/// A behaviour added to the tree by a user, which we convert to a a BehaviourNode tree internally
/// to run the tree. This is the template of the behaviour without all the internal runtime state.
///
/// Constuction is via static fns on Behave, so we can do the dynamic bundle stuff.
/// although this probably makes it hard to load the tree def from an asset file?
///
/// TODO: could have an existing entity task too, that we insert a Running component on to start it.
#[derive(Debug)]
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
    /// If, then
    Conditional(BoxedConditionSystem),
}

impl Behave {
    pub fn dynamic_spawn<T: Bundle + Clone>(bundle: T) -> Behave {
        Behave::DynamicEntity(DynamicBundel::new(bundle))
    }
    pub fn conditional<Marker>(system: impl IntoSystem<In<BehaveCtx>, bool, Marker>) -> Behave {
        Behave::Conditional(Box::new(IntoSystem::into_system(system)))
    }
}

/// A state wraps the behaviour, and is the node in our internal tree representation of the behaviour tree
/// One per Behave, with extra state bits.
#[derive(Debug)]
pub(crate) enum BehaveNode {
    Wait {
        start_time: Option<f32>,
        secs_to_wait: f32,
        status: Option<BehaveNodeStatus>,
    },
    SpawnTask {
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
        system: BoxedConditionSystem,
    },
}

#[derive(Clone, Debug)]
enum EntityTaskStatus {
    NotStarted,
    Started(Entity),
    Complete(bool),
}

impl BehaveNode {
    fn existing_status(&self) -> &Option<BehaveNodeStatus> {
        match self {
            BehaveNode::Conditional { status, .. } => status,
            BehaveNode::Wait { status, .. } => status,
            BehaveNode::SpawnTask { status, .. } => status,
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
            BehaveNode::Conditional { .. } => write!(f, "Conditional")?,
            BehaveNode::Wait { secs_to_wait, .. } => write!(f, "Wait({secs_to_wait})")?,
            BehaveNode::SpawnTask { task_status, .. } => write!(f, "SpawnTask({task_status:?})")?,
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
    pub(crate) fn new(behave: Behave) -> Self {
        match behave {
            Behave::Conditional(_) => panic!("fooo"),
            Behave::Wait(secs_to_wait) => Self::Wait {
                start_time: None,
                secs_to_wait,
                status: None,
            },
            Behave::DynamicEntity(bundle) => Self::SpawnTask {
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

fn tick_node(
    n: &mut NodeMut<BehaveNode>,
    time: &Res<Time>,
    ecmd: &mut EntityCommands<'_>,
    target_entity: Entity,
) -> BehaveNodeStatus {
    let bt_entity = ecmd.id();
    use BehaveNode::*;
    debug!("tick_node: {:?} = {:?}", n.id(), n.value());
    // short circuit nodes that have already got a result, or are blocked waiting for a trigger
    match n.value().existing_status() {
        None => {}
        Some(BehaveNodeStatus::Running) => {}
        Some(BehaveNodeStatus::AwaitingTrigger) => {}
        Some(BehaveNodeStatus::Success) => return BehaveNodeStatus::Success,
        Some(BehaveNodeStatus::Failure) => return BehaveNodeStatus::Failure,
    }
    match n.value() {
        Conditional { status, system } => BehaveNodeStatus::Running,
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
        SpawnTask {
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
        SpawnTask{ task_status: EntityTaskStatus::Started(_), .. } => unreachable!("Short circuit should prevent this while AwaitingTrigger"),
        // this is when a trigger has reported a result, and we need to process it and update status
        #[rustfmt::skip]
        SpawnTask {task_status: EntityTaskStatus::Complete(true), status, ..} => {
            *status = Some(BehaveNodeStatus::Success);
            BehaveNodeStatus::Success
        }
        #[rustfmt::skip]
        SpawnTask {task_status: EntityTaskStatus::Complete(false), status, ..} => {
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
