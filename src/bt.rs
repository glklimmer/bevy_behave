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
    app.add_systems(Update, tick_trees); //.run_if(on_timer(Duration::from_secs(1))));
    app.add_observer(on_bt_status_report);
}

// when we recieve a status report, we need to set the status of the node in the tree,
// but wait until the next tick to do anything.
fn on_bt_status_report(
    trigger: Trigger<BtStatusReport>,
    mut commands: Commands,
    mut q_bt: Query<&mut BehaviourTree, (With<BtAwaitingTrigger>, Without<BtTreeFinished>)>,
) {
    info!("Got status report: {:?}", trigger);
    let ctx = trigger.event().ctx();
    let Ok(mut bt) = q_bt.get_mut(ctx.bt_entity) else {
        error!("Failed to get bt entity on {}", trigger.entity());
        return;
    };
    // remove the waiting trigger component, so the tree will be ticked next time.
    commands.entity(ctx.bt_entity).remove::<BtAwaitingTrigger>();
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

/// The entity of the character that the behaviour is controlling
#[derive(Component, Debug, Default)]
pub enum TargetEntity {
    // uses the direct parent of the behaviour tree entity
    #[default]
    Parent,
    // uses a specified entity
    Entity(Entity),
}

/// If present, don't tick tree.
/// means tree is sleeping, until a trigger reports a status (which removes the component)
#[derive(Component)]
struct BtAwaitingTrigger;

#[derive(Component)]
struct BtTreeFinished;

#[allow(clippy::type_complexity)]
fn tick_trees(
    mut query: Query<
        (Entity, &mut BehaviourTree, Option<&Parent>, &TargetEntity),
        (Without<BtAwaitingTrigger>, Without<BtTreeFinished>),
    >,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (entity, mut bt, opt_parent, target_entity) in query.iter_mut() {
        let target_entity = match target_entity {
            TargetEntity::Parent => opt_parent.map(|p| p.get()).unwrap_or(Entity::PLACEHOLDER),
            TargetEntity::Entity(e) => *e,
        };
        let mut ecmd = commands.entity(entity);
        let res = bt.tick(&time, &mut ecmd, target_entity);
        match res {
            Status::AwaitingTrigger => {
                info!("tick_trees -> {:?}", res);
                ecmd.insert(BtAwaitingTrigger);
            }
            Status::Success | Status::Failure => {
                info!("tick_trees -> {:?}", res);
                ecmd.insert(BtTreeFinished);
            }
            Status::Running => {}
        }
    }
}

/// Inserted to SpawnTask entities, so they can report their status back to the tree.
/// and look up the agent entity they are controlling.
#[derive(Component, Debug, Copy, Clone)]
pub struct BtCtx {
    bt_entity: Entity,
    task_entity: Entity,
    target_entity: Entity,
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
    SequenceFlow,
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
        status: Option<Status>,
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
            Behaviour::SequenceFlow => Self::SequenceFlow { status: None },
            // Behaviour::FallbackFlow(behaviours) => Self::FallbackFlow {
            //     current_index: 0,
            //     current_state: Box::new(Self::new(behaviours[0].clone())),
            //     behaviours,
            // },
        }
    }
}

fn tick_node(
    n: &mut NodeMut<BehaviourNode>,
    time: &Res<Time>,
    ecmd: &mut EntityCommands<'_>,
    target_entity: Entity,
) -> Status {
    let bt_entity = ecmd.id();
    use BehaviourNode::*;
    info!("tick_node: {:?} = {:?}", n.id(), n.value());
    match n.value() {
        // start waiting
        Wait {
            start_time: start_time @ None,
            secs_to_wait: _,
        } => {
            info!("Starting wait");
            *start_time = Some(time.elapsed_secs());
            Status::Running
        }
        // continue waiting
        Wait {
            start_time: Some(start_time),
            secs_to_wait,
        } => {
            // info!("Waiting");
            let elapsed = time.elapsed_secs() - *start_time;
            if elapsed > *secs_to_wait {
                return Status::Success;
            }
            Status::Running
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
            let ctx = BtCtx {
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
            *status = Some(Status::AwaitingTrigger);
            Status::AwaitingTrigger
        }
        // we're still waiting on an entity to trigger a result.
        // SpawnTask {
        //     entity: Some(entity),
        //     status,
        //     bundle: _,
        // } => {
        //     info!("SpawnTask. Entity running");
        //     // usually Running, but maybe a trigger poked in a new status here:
        //     let Some(status) = status else {
        //         panic!("Invalid state: spawntask has entity but no status");
        //     };
        //     *status
        // }
        // run a sequence of behaviours
        SequenceFlow {
            status: Some(status),
            ..
        } if matches!(status, Status::Success | Status::Failure) => {
            info!("SequenceFlow. Returning existing status: {:?}", status);
            *status
        }
        // don't bind any fields here because we need to mutably borrow the node again
        SequenceFlow { .. } => {
            // info!("SequenceFlow. Processing children");
            let Some(mut child) = n.first_child() else {
                warn!("SequenceFlow with no children, returning success anyway");
                return Status::Success;
            };

            let mut final_status;
            loop {
                // info!(
                //     "calling tick for a child of a sequenceflow: {:?}",
                //     child.id()
                // );
                // std::thread::sleep(Duration::from_secs(1));
                match tick_node(&mut child, time, ecmd, target_entity) {
                    Status::AwaitingTrigger => {
                        final_status = Status::AwaitingTrigger;
                        break;
                    }
                    Status::Running => {
                        final_status = Status::Running;
                        break;
                    }
                    Status::Failure => {
                        final_status = Status::Failure;
                        break;
                    }
                    Status::Success => {
                        final_status = Status::Success;
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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Success,
    Failure,
    Running,
    AwaitingTrigger,
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
#[require(TargetEntity)]
pub struct BehaviourTree {
    tree: Tree<BehaviourNode>,
    // entity: Entity,
}

impl BehaviourTree {
    pub fn new(tree: Tree<Behaviour>) -> Self {
        // convert to internal BehaviourNode tree
        let tree = tree.map(BehaviourNode::new);
        info!("BehaviourTree. New tree: {:?}", tree);
        Self {
            tree,
            // entity: Entity::PLACEHOLDER,
        }
    }

    // fn set_entity(&mut self, entity: Entity) {
    //     self.entity = entity;
    // }

    fn tick(
        &mut self,
        time: &Res<Time>,
        ecmd: &mut EntityCommands<'_>,
        target_entity: Entity,
    ) -> Status {
        let mut node = self.tree.root_mut();
        tick_node(&mut node, time, ecmd, target_entity)
    }
    // sets the status of a spawn task node, so it should progress next tick.
    fn set_node_result(&mut self, entity: Entity, new_status: Status) {
        // find the node that is a SpawnTask matching this entity:
        let node_id = self
            .tree
            .nodes()
            .find(|n| match n.value() {
                BehaviourNode::SpawnTask {
                    entity: Some(e), ..
                } => *e == entity,
                _ => false,
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
