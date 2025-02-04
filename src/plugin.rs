use crate::{
    prelude::*, tick_node, BehaveNode, BehaveNodeStatus, EntityTaskStatus, TriggerTaskStatus,
};
use bevy::prelude::*;
// use bevy::app::FixedPreUpdate;
use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::{ScheduleLabel, SystemSet};
use ego_tree::*;

/// The `BehaveTree` components are ticked in this set, which is configured into the schedule
/// provided to the `BehavePlugin`. This defaults to `FixedPreUpdate`.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct BehaveSet;

/// Plugin to tick the `BehaveTree` components.
/// Defaults to configuring the `BehaveSet` to run in `FixedPreUpdate`.
pub struct BehavePlugin {
    pub schedule: Interned<dyn ScheduleLabel>,
}

impl BehavePlugin {
    /// Run the BehaveTree tick system in this schedule
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
        }
    }
}

impl Default for BehavePlugin {
    /// Defaults to `FixedPreUpdate`.
    fn default() -> Self {
        Self::new(FixedPreUpdate)
    }
}

impl Plugin for BehavePlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(self.schedule, BehaveSet);
        app.add_systems(
            self.schedule,
            tick_trees
                // .run_if(bevy::time::common_conditions::on_timer(
                //     std::time::Duration::from_secs(1),
                // ))
                .in_set(BehaveSet),
        );
        // adds a global observer to listen for status report events
        app.add_plugins(crate::ctx::plugin);
    }
}

/// The entity of the character that the behaviour is controlling.
/// This is required to be on the entity holding the BehaviourTree component.
/// The actual entity (either specified here or the parent) is provided by calling
/// `ctx.target_entity()` from the ctx component or trigger event.
/// TODO: constructor fns and private impl to avoid confusion?
#[derive(Component, Debug, Default)]
pub enum BehaveTargetEntity {
    // uses the direct parent of the behaviour tree entity
    #[default]
    Parent,
    // uses a specified entity
    Entity(Entity),
}

/// If present on the BehaveTree entity, don't tick tree.
/// Means tree is sleeping, until a trigger reports a status (which removes the component).
#[derive(Component)]
pub(crate) struct BehaveAwaitingTrigger;

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
        info!("ABOUT TO TICK bt: {}", *bt);
        let res = bt.tick(&time, &mut ecmd, target_entity);
        info!("\n{}", *bt);
        match res {
            BehaveNodeStatus::AwaitingTrigger => {
                // info!("tick_trees -> {:?}", res);
                ecmd.insert(BehaveAwaitingTrigger);
            }
            BehaveNodeStatus::Success => {
                // info!("tick_trees -> {:?}", res);
                ecmd.insert(BehaveFinished(true));
            }
            BehaveNodeStatus::Failure => {
                // info!("tick_trees -> {:?}", res);
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
impl std::fmt::Display for BehaveTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        walk_tree(self.tree.root(), 0, f)?;
        Ok(())
    }
}

fn walk_tree(
    node: NodeRef<BehaveNode>,
    depth: usize,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    for _ in 0..(depth * 2) {
        write!(f, " ")?;
    }
    write!(f, "* ")?;
    writeln!(f, "{}  [{:?}]", node.value(), node.id())?;
    for child in node.children() {
        walk_tree(child, depth + 1, f)?;
    }
    Ok(())
}

impl BehaveTree {
    pub fn new(tree: Tree<Behave>) -> Self {
        // convert to internal BehaveNode tree
        let tree = tree.map(BehaveNode::new);
        Self { tree }
    }

    fn tick(
        &mut self,
        time: &Res<Time>,
        ecmd: &mut EntityCommands<'_>,
        target_entity: Entity,
    ) -> BehaveNodeStatus {
        let mut node = self.tree.root_mut();
        tick_node(&mut node, time, ecmd, target_entity)
    }

    /// Returns Option<Entity> being an entity that was spawned to run this task node.
    /// (so it can be despawned now that the task is complete)
    /// Will always be none if reporting a result from a TriggerReq node.
    pub(crate) fn set_node_result(&mut self, ctx: &BehaveCtx, success: bool) -> Option<Entity> {
        let node_id = ctx.task_node();
        let mut node = self.tree.get_mut(node_id).unwrap();
        let val = node.value();
        match val {
            BehaveNode::DynamicEntity { task_status, .. } if ctx.is_for_entity() => {
                // extract the entity that was running this node, so we can despawn it
                let task_entity = match task_status {
                    EntityTaskStatus::Started(e) => Some(*e),
                    _ => {
                        warn!("Given node ({node_id:?}) result for a non-spawned entity node?");
                        None
                    }
                };
                info!(
                    "Setting conditional task for {node_id:?} success to {:?}",
                    success
                );
                *task_status = EntityTaskStatus::Complete(success);
                task_entity
            }
            BehaveNode::TriggerReq { task_status, .. } => {
                info!(
                    "Setting conditional task for {node_id:?} success to {:?}",
                    success
                );
                *task_status = TriggerTaskStatus::Complete(success);
                None
            }
            _ => {
                error!("Given node result but no matching node found: {node_id:?}");
                None
            }
        }
    }
}
