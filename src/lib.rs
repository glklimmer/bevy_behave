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

/// A behaviour added to the tree by a user, which we convert to a a BehaviourNode tree internally
/// to run the tree. This is the template of the behaviour without all the internal runtime state.
///
/// Constuction is via static fns on Behave, so we can do the dynamic bundle stuff.
/// although this probably makes it hard to load the tree def from an asset file?
///
/// TODO: could have an existing entity task too, that we insert a Running component on to start it.
#[derive(Clone, Debug)]
pub enum Behave {
    /// Waits this many seconds before Succeeding
    Wait(f32),
    /// Spawns an entity, and waits for it to trigger a status report
    /// Use the Behaviour::spawn_entity constructor, or import dyn_bundle (not in prelude)
    DynamicEntity(DynamicBundel),
    /// Runs children in sequence, failing if any fails, succeeding if all succeed
    SequenceFlow,
    // FallbackFlow(Vec<Behaviour>),
}

impl Behave {
    pub fn spawn_entity<T: Bundle + Clone>(bundle: T) -> Behave {
        Behave::DynamicEntity(DynamicBundel::new(bundle))
    }
}

/// A state wraps the behaviour, and is the node in our internal tree representation of the behaviour tree
/// One per Behave, with extra state bits.
#[derive(Clone, Debug)]
pub(crate) enum BehaveNode {
    Wait {
        start_time: Option<f32>,
        secs_to_wait: f32,
    },
    SpawnTask {
        // None until something spawned.
        entity: Option<Entity>,
        status: Option<BehaveNodeStatus>,
        bundle: DynamicBundel,
    },
    SequenceFlow {
        status: Option<BehaveNodeStatus>,
    },
    // FallbackFlow {
    //     behaviours: Vec<Behaviour>,
    //     current_index: usize,
    //     current_state: Box<BehaviourNode>,
    // },
}

impl BehaveNode {
    pub(crate) fn new(behave: Behave) -> Self {
        match behave {
            Behave::Wait(secs_to_wait) => Self::Wait {
                start_time: None,
                secs_to_wait,
            },
            Behave::DynamicEntity(bundle) => Self::SpawnTask {
                entity: None,
                status: None,
                bundle,
            },
            Behave::SequenceFlow => Self::SequenceFlow { status: None },
            // Behaviour::FallbackFlow(behaviours) => Self::FallbackFlow {
            //     current_index: 0,
            //     current_state: Box::new(Self::new(behaviours[0].clone())),
            //     behaviours,
            // },
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
    info!("tick_node: {:?} = {:?}", n.id(), n.value());
    match n.value() {
        // start waiting
        Wait {
            start_time: start_time @ None,
            secs_to_wait: _,
        } => {
            info!("Starting wait");
            *start_time = Some(time.elapsed_secs());
            BehaveNodeStatus::Running
        }
        // continue waiting
        Wait {
            start_time: Some(start_time),
            secs_to_wait,
        } => {
            // info!("Waiting");
            let elapsed = time.elapsed_secs() - *start_time;
            if elapsed > *secs_to_wait {
                return BehaveNodeStatus::Success;
            }
            BehaveNodeStatus::Running
        }
        // a spawntask with a status has been started already:
        SpawnTask {
            status: Some(status),
            ..
        } => {
            info!("SpawnTask with existing status: {:?}", status);
            *status
        }
        // spawn a new entity for this task
        SpawnTask {
            status: status @ None,
            entity,
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
            *entity = Some(id);
            *status = Some(BehaveNodeStatus::AwaitingTrigger);
            BehaveNodeStatus::AwaitingTrigger
        }
        // run a sequence of behaviours
        SequenceFlow {
            status: Some(status),
            ..
        } if matches!(
            status,
            BehaveNodeStatus::Success | BehaveNodeStatus::Failure
        ) =>
        {
            info!("SequenceFlow. Returning existing status: {:?}", status);
            *status
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
                    BehaveNodeStatus::AwaitingTrigger => {
                        final_status = BehaveNodeStatus::AwaitingTrigger;
                        break;
                    }
                    BehaveNodeStatus::Running => {
                        final_status = BehaveNodeStatus::Running;
                        break;
                    }
                    BehaveNodeStatus::Failure => {
                        final_status = BehaveNodeStatus::Failure;
                        break;
                    }
                    BehaveNodeStatus::Success => {
                        final_status = BehaveNodeStatus::Success;
                        if let Ok(next_child) = child.into_next_sibling() {
                            child = next_child;
                            continue;
                        } else {
                            break;
                        }
                    }
                }
            }
            let SequenceFlow { status, .. } = n.value() else {
                unreachable!("Must be a SequenceFlow");
            };
            // info!("SequenceFlow. Setting status to {:?}", final_status);
            *status = Some(final_status);
            final_status
        }
    }
}
