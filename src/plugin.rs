use crate::*;
use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::ScheduleLabel;
use bevy::prelude::*;
use dyn_bundle::prelude::*;
use ego_tree::*;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct BehaveSet;

pub struct BehavePlugin {
    pub schedule: Interned<dyn ScheduleLabel>,
}

impl BehavePlugin {
    /// Defaults to FixedUpdate, or provide the schedule to run the tree ticking in.
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
        }
    }
}

impl Default for BehavePlugin {
    fn default() -> Self {
        Self::new(FixedUpdate)
    }
}

impl Plugin for BehavePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(self.schedule, BehaveSet);
        app.add_systems(self.schedule, tick_trees.in_set(BehaveSet));
        app.add_observer(on_bt_status_report);
    }
}

/// The entity of the character that the behaviour is controlling.
/// This is required to be on the entity holding the BehaviourTree component.
#[derive(Component, Debug, Default)]
pub enum BehaveTargetEntity {
    // uses the direct parent of the behaviour tree entity
    #[default]
    Parent,
    // uses a specified entity
    Entity(Entity),
}

/// Inserted to SpawnTask entities, so they can report their status back to the tree.
/// and look up the agent entity they are controlling.
#[derive(Component, Debug, Copy, Clone)]
pub struct BehaveCtx {
    pub(crate) bt_entity: Entity,
    pub(crate) task_entity: Entity,
    pub(crate) target_entity: Entity,
}

impl BehaveCtx {
    pub fn success(&self) -> BehaveStatusReport {
        BehaveStatusReport::Success(*self)
    }
    pub fn failure(&self) -> BehaveStatusReport {
        BehaveStatusReport::Failure(*self)
    }
    pub fn target_entity(&self) -> Entity {
        self.target_entity
    }
    pub fn behave_entity(&self) -> Entity {
        self.bt_entity
    }
    pub fn task_entity(&self) -> Entity {
        self.task_entity
    }
}

// contains the bt entity to update
#[derive(Debug, Event)]
pub enum BehaveStatusReport {
    Success(BehaveCtx),
    Failure(BehaveCtx),
}

impl BehaveStatusReport {
    pub fn ctx(&self) -> &BehaveCtx {
        match self {
            BehaveStatusReport::Success(ctx) => ctx,
            BehaveStatusReport::Failure(ctx) => ctx,
        }
    }
}

// when we recieve a status report, we need to set the status of the node in the tree,
// but wait until the next tick to do anything.
fn on_bt_status_report(
    trigger: Trigger<BehaveStatusReport>,
    mut commands: Commands,
    mut q_bt: Query<&mut BehaveTree, (With<BehaveAwaitingTrigger>, Without<BehaveFinished>)>,
) {
    info!("Got status report: {:?}", trigger);
    let ctx = trigger.event().ctx();
    let Ok(mut bt) = q_bt.get_mut(ctx.bt_entity) else {
        error!("Failed to get bt entity on {}", trigger.entity());
        return;
    };
    // remove the waiting trigger component, so the tree will be ticked next time.
    commands
        .entity(ctx.bt_entity)
        .remove::<BehaveAwaitingTrigger>();
    // despawn the entity used for this task now that it is complete.
    commands.entity(ctx.task_entity).try_despawn_recursive();
    match trigger.event() {
        BehaveStatusReport::Success(ctx) => {
            bt.set_node_result(ctx.task_entity, BehaveNodeStatus::Success);
        }
        BehaveStatusReport::Failure(ctx) => {
            bt.set_node_result(ctx.task_entity, BehaveNodeStatus::Failure);
        }
    }
}

#[allow(clippy::type_complexity)]
fn tick_trees(
    mut query: Query<
        (
            Entity,
            &mut BehaveTree,
            Option<&Parent>,
            &BehaveTargetEntity,
        ),
        (Without<BehaveAwaitingTrigger>, Without<BehaveFinished>),
    >,
    mut commands: Commands,
    time: Res<Time>,
) {
    for (entity, mut bt, opt_parent, target_entity) in query.iter_mut() {
        let target_entity = match target_entity {
            BehaveTargetEntity::Parent => {
                opt_parent.map(|p| p.get()).unwrap_or(Entity::PLACEHOLDER)
            }
            BehaveTargetEntity::Entity(e) => *e,
        };
        let mut ecmd = commands.entity(entity);
        let res = bt.tick(&time, &mut ecmd, target_entity);
        match res {
            BehaveNodeStatus::AwaitingTrigger => {
                info!("tick_trees -> {:?}", res);
                ecmd.insert(BehaveAwaitingTrigger);
            }
            BehaveNodeStatus::Success => {
                info!("tick_trees -> {:?}", res);
                ecmd.insert(BehaveFinished(true));
            }
            BehaveNodeStatus::Failure => {
                info!("tick_trees -> {:?}", res);
                ecmd.insert(BehaveFinished(false));
            }
            BehaveNodeStatus::Running => {}
        }
    }
}

#[derive(Component)]
#[require(BehaveTargetEntity)]
pub struct BehaveTree {
    tree: Tree<BehaveNode>,
}

impl BehaveTree {
    pub fn new(tree: Tree<Behave>) -> Self {
        // convert to internal BehaviourNode tree
        let tree = tree.map(BehaveNode::new);
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
    ) -> BehaveNodeStatus {
        let mut node = self.tree.root_mut();
        tick_node(&mut node, time, ecmd, target_entity)
    }
    // sets the status of a spawn task node, so it should progress next tick.
    fn set_node_result(&mut self, entity: Entity, new_status: BehaveNodeStatus) {
        // find the node that is a SpawnTask matching this entity:
        let node_id = self
            .tree
            .nodes()
            .find(|n| match n.value() {
                BehaveNode::SpawnTask {
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
            BehaveNode::SpawnTask { status, .. } => {
                info!("Setting spawn task status to {:?}", new_status);
                *status = Some(new_status);
            }
            _ => {
                warn!("Given node result for a non-spawntask node?");
            }
        }
    }
}
