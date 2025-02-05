use bevy::prelude::*;
use ego_tree::*;

mod behave_trigger;
mod ctx;
pub mod dyn_bundle;
mod plugin;

use behave_trigger::*;
use ctx::*;
use dyn_bundle::prelude::*;

// in case users want to construct the tree without using the macro, we reexport:
pub use ego_tree;

/// Includes the ego_tree `tree!` macro for easy tree construction.
/// this crate also re-exports `ego_tree` so you can construct trees manually (but not in prelude).
pub mod prelude {
    pub use super::behave;
    pub use super::behave_trigger::BehaveTrigger;
    pub use super::ctx::*;
    pub use super::plugin::*;
    pub use super::{Behave, BehaveFinished};
    pub use ego_tree::*;
}

/// A node on the behave tree can be in one of these states
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BehaveNodeStatus {
    Success,
    Failure,
    Running,
    AwaitingTrigger,
}

/// Inserted on the entity with the BehaveTree when the tree has finished executing.
/// Containts the final result of the tree.
#[derive(Component, Reflect, Debug)]
pub struct BehaveFinished(pub bool);

/// A behaviour added to the tree by a user, which we convert to a a BehaviourNode tree internally
/// to run the tree. This is the template of the behaviour without all the internal runtime state.
///
/// Constuction is via static fns on Behave, so we can do the dynamic bundle stuff.
/// although this probably makes it hard to load the tree def from an asset file?
#[derive(Clone)]
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
    /// Returns a result from a trigger. Can be used as a conditional (returning success or failure)
    /// or simply to execute some bevy systems code without spawning an entity.
    TriggerReq(DynamicTrigger),
    /// Loops forever
    Forever,
}

impl std::fmt::Display for Behave {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Behave::Wait(secs) => write!(f, "Wait({secs}s)"),
            Behave::DynamicEntity(_) => write!(f, "DynamicEntity"),
            Behave::Sequence => write!(f, "Sequence"),
            Behave::Fallback => write!(f, "Fallback"),
            Behave::Invert => write!(f, "Invert"),
            Behave::AlwaysSucceed => write!(f, "AlwaysSucceed"),
            Behave::AlwaysFail => write!(f, "AlwaysFail"),
            Behave::TriggerReq(t) => write!(f, "TriggerReq({})", t.type_name()),
            Behave::Forever => write!(f, "Forever"),
        }
    }
}

impl Behave {
    pub fn dynamic_spawn<T: Bundle + Clone>(bundle: T) -> Behave {
        Behave::DynamicEntity(DynamicBundel::new(bundle))
    }
    pub fn trigger_req<T: Clone + Send + Sync + 'static>(value: T) -> Self {
        Behave::TriggerReq(DynamicTrigger::new(value))
    }
}

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
    TriggerReq {
        status: Option<BehaveNodeStatus>,
        task_status: TriggerTaskStatus,
        trigger: DynamicTrigger,
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
            BehaveNode::TriggerReq { status, .. } => status,
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
            BehaveNode::TriggerReq { trigger, .. } => write!(f, "TriggerReq({})", trigger.type_name())?,
            BehaveNode::Wait { secs_to_wait, .. } => write!(f, "Wait({secs_to_wait})")?,
            BehaveNode::DynamicEntity { .. } => write!(f, "SpawnTask")?,
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
            BehaveNode::TriggerReq {
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
            Behave::TriggerReq(trig_fn) => Self::TriggerReq {
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

// sucks there aren't good traversal fns on NodeMut like there are on NodeRef..
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
    commands: &mut Commands,
    bt_entity: Entity,
    target_entity: Entity,
) -> BehaveNodeStatus {
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
            match tick_node(&mut only_child, time, commands, bt_entity, target_entity) {
                BehaveNodeStatus::Success | BehaveNodeStatus::Failure => {
                    // reset so we can run it again next tick
                    reset_descendants(&mut only_child);
                    BehaveNodeStatus::Running
                }
                other => other,
            }
        }
        TriggerReq {
            task_status: task_status @ TriggerTaskStatus::NotTriggered,
            status,
            trigger,
        } => {
            let ctx = BehaveCtx::new_for_trigger(bt_entity, task_node, target_entity);
            commands.dyn_trigger(trigger.clone(), ctx);
            // Don't use AwaitingTrigger for this, because of ordering issues..
            // the trigger response arrives BEFORE we insert the BehaveAwaitingTrigger component,
            // so the trigger response handler can't remove it, so it never ticks.
            *task_status = TriggerTaskStatus::Triggered;
            *status = Some(BehaveNodeStatus::Running);
            BehaveNodeStatus::Running
        }
        #[rustfmt::skip]
        TriggerReq {task_status: TriggerTaskStatus::Complete(true), status, ..} => {
            *status = Some(BehaveNodeStatus::Success);
            BehaveNodeStatus::Success
        }
        #[rustfmt::skip]
        TriggerReq {task_status: TriggerTaskStatus::Complete(false), status, ..} => {
            *status = Some(BehaveNodeStatus::Failure);
            BehaveNodeStatus::Failure
        }
        #[rustfmt::skip]
        TriggerReq {task_status: TriggerTaskStatus::Triggered, status, .. } => {
            unreachable!(
                "Should have short circuited while awaiting trigger? returning: {:?}",
                status.unwrap()
            )
        }
        Invert { .. } => {
            let mut only_child = n.first_child().expect("Invert nodes must have a child");
            if only_child.has_siblings() {
                panic!("Invert nodes must have a single child, not multiple children");
            }
            let res = match tick_node(&mut only_child, time, commands, bt_entity, target_entity) {
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
            // info!("Starting wait");
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
            // bit of extra archetype moving here for now, but i need the ctx to be on the entity
            // before the user's components, so it can be found in an OnAdd trigger.
            // might change dyn_insert to allow extra component at insertion time..
            let mut e = commands.spawn(());
            e.set_parent(bt_entity);
            let ctx = BehaveCtx::new_for_entity(bt_entity, task_node, target_entity);
            // NB: if the component in the dyn bundle has an OnAdd which reports success or failure
            //     immediately, the entity will be despawned instantly, so you can't do something
            //     like .set_parent on it after doing the insertion (we set_parent above).
            //     Else you get a "The entity with ID X does not exist" panic in bevy_hierarchy code.
            let id = e.insert(ctx).dyn_insert(bundle.clone()).id();
            // info!("Spawned entity: {id:?} (parent: {bt_entity:?}) for node {task_node:?}",);
            *task_status = EntityTaskStatus::Started(id);
            // We go to Running for one tick, so that any OnAdd trigger that immediately reports a
            // status we take effect properly.
            // Next match case will set to AwaitingTrigger if we don't get a status report
            // Otherwise there is an ordering mismatch and the AwaitingTrigger isn't removed.
            *status = Some(BehaveNodeStatus::Running);
            BehaveNodeStatus::Running
        }
        #[rustfmt::skip]
        DynamicEntity { task_status: EntityTaskStatus::Started(_), status: status @ Some(BehaveNodeStatus::Running), .. } => {
            // if we tick without having received a status report, it means there can't have been any OnAdd trigger 
            // that immediately sent a report and caused a despawn, so we can go dormant:
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
                match tick_node(&mut child, time, commands, bt_entity, target_entity) {
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
                match tick_node(&mut child, time, commands, bt_entity, target_entity) {
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

/// Modifed version of ego_tree's tree! macro, to allow merging subtrees:
///
/// let subtree: Tree<Behave> = get_subtree();
/// let t = tree! {
///     Behave::Sequence => {
///         Behave::Wait(2),
///         @ subtree
///     }
/// };
///
#[macro_export]
macro_rules! behave {
    // Use an “@” marker to indicate that the expression is a subtree, to be merged into the tree.
    (@ $n:ident { @ $subtree:expr $(, $($tail:tt)*)? }) => {{
        $n.append_subtree($subtree);
        $( behave!(@ $n { $($tail)* }); )?
    }};

    // Base case: no tokens left.
    (@ $n:ident { }) => { };

    // Leaf: last value.
    (@ $n:ident { $value:expr }) => {{
        $n.append($value);
    }};

    // Leaf: value with additional siblings.
    (@ $n:ident { $value:expr, $($tail:tt)* }) => {{
        $n.append($value);
        behave!(@ $n { $($tail)* });
    }};

    // Node: last node with children.
    (@ $n:ident { $value:expr => $children:tt }) => {{
        let mut node = $n.append($value);
        behave!(@ node $children);
    }};

    // Node: node with children and additional siblings.
    (@ $n:ident { $value:expr => $children:tt, $($tail:tt)* }) => {{
        let mut node = $n.append($value);
        behave!(@ node $children);
        behave!(@ $n { $($tail)* });
    }};

    // Top-level: tree with a root only.
    ($root:expr) => { $crate::ego_tree::Tree::new($root) };

    // Top-level: tree with a root and children.
    ($root:expr => $children:tt) => {{
        let mut tree = $crate::ego_tree::Tree::new($root);
        {
            let mut node = tree.root_mut();
            behave!(@ node $children);
        }
        tree
    }};
}
