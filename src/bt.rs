use crate::dyn_bundle::prelude::*;
use bevy::{prelude::*, time::common_conditions::on_timer, utils::HashMap};
use ego_tree::*;
use std::time::Duration;

pub fn bt_plugin(app: &mut App) {
    // app.add_observer(on_input.pipe(handle_outputs));
    // app.add_systems(
    //     Update,
    //     process_next_node.run_if(on_timer(Duration::from_secs(1))),
    // );
    app.add_systems(Update, tick_trees.run_if(on_timer(Duration::from_secs(1))));
    app.add_observer(on_bt_added);
    app.add_observer(on_bt_status_report);
}

fn on_bt_added(trigger: Trigger<OnAdd, BehaviourTree>, mut q: Query<&mut BehaviourTree>) {
    let mut bt = q.get_mut(trigger.entity()).unwrap();
    info!("Setting bt entity: {:?}", trigger.entity());
    bt.set_entity(trigger.entity());
}

// when we recieve a status report, we need to set the status of the node in the tree,
// but wait until the next tick to do anything.
fn on_bt_status_report(
    trigger: Trigger<BtStatusReport>,
    mut commands: Commands,
    q: Query<&BtCtx>,
    mut q_bt: Query<&mut BehaviourTree>,
) {
    info!("Got status report: {:?}", trigger);
    let ctx = trigger.event().ctx();
    let Ok(mut bt) = q_bt.get_mut(ctx.bt_entity) else {
        error!("Failed to get bt entity on {}", trigger.entity());
        return;
    };
    // despawn the entity used for this task now that it is complete.
    commands.entity(ctx.task_entity).try_despawn_recursive();
    match trigger.event() {
        BtStatusReport::Success(ctx) => {
            bt.set_node_result(ctx.task_entity, Status::Success);
        }
        BtStatusReport::Failure(ctx) => {
            bt.set_node_result(ctx.task_entity, Status::Failure);
        }
    }
}

fn tick_trees(
    mut query: Query<(Entity, &mut BehaviourTree)>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (entity, mut bt) in query.iter_mut() {
        let mut commands = commands.entity(entity);
        let res = bt.tick(&time, &mut commands);
        info!("tick_trees: {:?}", res);
    }
}

#[derive(Component, Debug, Copy, Clone)]
pub struct BtCtx {
    bt_entity: Entity,
    task_entity: Entity,
    // agent_entity: Entity,
}

impl BtCtx {
    pub fn success(&self) -> BtStatusReport {
        BtStatusReport::Success(*self)
    }
    pub fn failure(&self) -> BtStatusReport {
        BtStatusReport::Failure(*self)
    }
}

/// A behaviour added to the tree by a user, which we convert to a a BehaviourNode tree internally
/// to run the tree. This is the template of the behaviour without all the internal runtime state.
#[derive(Clone, Debug)]
pub enum Behaviour {
    Wait(f32),
    SpawnTask(DynamicBundel),
    SequenceFlow(Vec<Behaviour>),
    // FallbackFlow(Vec<Behaviour>),
}

/// A state wraps the behaviour, and is the node in our internal tree representation of the behaviour tree
/// One per Behaviour, with extra state bits.
#[derive(Clone, Debug)]
enum BehaviourNode {
    Wait {
        start_time: Option<f32>,
        secs_to_wait: f32,
    },
    SpawnTask {
        // None until something spawned.
        entity: Option<Entity>,
        status: Option<Status>,
        bundle: DynamicBundel,
    },
    SequenceFlow {
        behaviours: Vec<Behaviour>,
        status: Option<Status>,
        current_index: usize,
        current_state: Box<BehaviourNode>,
    },
    // FallbackFlow {
    //     behaviours: Vec<Behaviour>,
    //     current_index: usize,
    //     current_state: Box<BehaviourNode>,
    // },
}

impl BehaviourNode {
    fn new(behaviour: Behaviour) -> Self {
        match behaviour {
            Behaviour::Wait(secs_to_wait) => Self::Wait {
                start_time: None,
                secs_to_wait,
            },
            Behaviour::SpawnTask(bundle) => Self::SpawnTask {
                entity: None,
                status: None,
                bundle,
            },
            Behaviour::SequenceFlow(behaviours) => Self::SequenceFlow {
                current_index: 0,
                current_state: Box::new(Self::new(behaviours[0].clone())),
                behaviours,
                status: None,
            },
            // Behaviour::FallbackFlow(behaviours) => Self::FallbackFlow {
            //     current_index: 0,
            //     current_state: Box::new(Self::new(behaviours[0].clone())),
            //     behaviours,
            // },
        }
    }

    fn tick(&mut self, time: &Res<Time>, commands: &mut EntityCommands<'_>) -> Status {
        let bt_entity = commands.id();
        match self {
            // start waiting
            Self::Wait {
                start_time: start_time @ None,
                secs_to_wait: _,
            } => {
                info!("Starting wait");
                *start_time = Some(time.elapsed_secs());
                Status::Running
            }
            // continue waiting
            Self::Wait {
                start_time: Some(start_time),
                secs_to_wait,
            } => {
                info!("Waiting");
                let elapsed = time.elapsed_secs() - *start_time;
                if elapsed > *secs_to_wait {
                    return Status::Success;
                }
                Status::Running
            }
            // a spawntask with a status has been started already:
            Self::SpawnTask {
                status: Some(status),
                ..
            } => *status,
            // spawn a new entity for this task
            Self::SpawnTask {
                entity: entity @ None,
                status,
                bundle,
            } => {
                let id = commands.commands().dyn_spawn(bundle.clone()).id();
                commands.commands().entity(id).insert(BtCtx {
                    bt_entity,
                    task_entity: id,
                });
                info!("Spawning entity: {id:?}");
                commands.add_child(id);
                *entity = Some(id);
                *status = Some(Status::Running);
                Status::Running
            }
            // we're still waiting on an entity to trigger a result.
            Self::SpawnTask {
                entity: Some(entity),
                status,
                bundle: _,
            } => {
                info!("Entity running");
                // usually Running, but maybe a trigger poked in a new status here:
                let Some(status) = status else {
                    panic!("Invalid state: spawntask has entity but no status");
                };
                *status
            }
            // run a sequence of behaviours
            Self::SequenceFlow {
                status: Some(status),
                ..
            } => *status,
            Self::SequenceFlow {
                status: status @ None,
                behaviours: seq,
                current_index: idx,
                current_state: cursor,
            } => {
                while *idx < seq.len() {
                    match cursor.tick(time, commands) {
                        Status::Running => {
                            return Status::Running;
                        }
                        Status::Success => {
                            // child succeeded, move to next node
                            *idx += 1;
                        }
                        Status::Failure => {
                            *status = Some(Status::Failure);
                            return Status::Failure;
                        }
                    }
                    if *idx >= seq.len() {
                        *status = Some(Status::Success);
                        return Status::Success;
                    }
                    // progress to next node
                    **cursor = BehaviourNode::new(seq[*idx].clone());
                }
                unreachable!("Shouldn't get here?");
            }
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Success,
    Failure,
    Running,
}

// contains the bt entity to update
#[derive(Debug, Event)]
pub enum BtStatusReport {
    Success(BtCtx),
    Failure(BtCtx),
}

impl BtStatusReport {
    pub fn ctx(&self) -> &BtCtx {
        match self {
            BtStatusReport::Success(ctx) => ctx,
            BtStatusReport::Failure(ctx) => ctx,
        }
    }
}

#[derive(Component)]
pub struct BehaviourTree {
    tree: Tree<BehaviourNode>,
    entity: Entity,
}

impl BehaviourTree {
    pub fn new(tree: Tree<Behaviour>) -> Self {
        // convert to internal BehaviourNode tree
        let tree = tree.map(BehaviourNode::new);
        Self {
            tree,
            entity: Entity::PLACEHOLDER,
        }
    }

    fn set_entity(&mut self, entity: Entity) {
        self.entity = entity;
    }

    fn tick(&mut self, time: &Res<Time>, commands: &mut EntityCommands<'_>) -> Status {
        let mut node = self.tree.root_mut();
        let val = node.value();
        val.tick(time, commands)
    }
    // sets the status of a spawn task node, so it should progress next tick.
    fn set_node_result(&mut self, entity: Entity, new_status: Status) {
        // find the node that is a SpawnTask matching this entity:
        let node_id = self
            .tree
            .nodes()
            .find(|n| {
                info!("Testing {:?} {:?}", n.id(), n.value());
                match n.value() {
                    BehaviourNode::SpawnTask {
                        entity: Some(e), ..
                    } => *e == entity,
                    _ => false,
                }
            })
            .map(|n| n.id());
        let Some(node_id) = node_id else {
            warn!("Given node result for a non-spawntask entity: {entity:?} ?");
            return;
        };
        let mut node = self.tree.get_mut(node_id).unwrap();
        let val = node.value();
        match val {
            BehaviourNode::SpawnTask { status, .. } => {
                info!("Setting spawn task status to {:?}", new_status);
                *status = Some(new_status);
            }
            _ => {
                warn!("Given node result for a non-spawntask node?");
            }
        }
    }
}
