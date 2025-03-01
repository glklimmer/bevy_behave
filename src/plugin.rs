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
    schedule: Interned<dyn ScheduleLabel>,
}

impl BehavePlugin {
    /// Run the BehaveTree tick system in this schedule
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self {
            schedule: schedule.intern(),
        }
    }
    /// Return the schedule this plugin will run in.
    pub fn schedule(&self) -> &Interned<dyn ScheduleLabel> {
        &self.schedule
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
        app.register_type::<BehaveTimeout>();
        app.add_systems(
            self.schedule,
            (tick_timeout_components, tick_trees)
                .chain()
                .in_set(BehaveSet),
        );
        app.add_observer(on_tick_timeout_added);
        // adds a global observer to listen for status report events
        app.add_plugins(crate::ctx::plugin);
    }
}

/// The entity of the character that the behaviour is controlling.
/// This is required to be on the entity holding the BehaviourTree component.
/// The actual entity (either specified here or the parent) is provided by calling
/// `ctx.target_entity()` from the ctx component or trigger event.
#[derive(Component, Debug, Default)]
pub enum BehaveTargetEntity {
    /// Uses the direct parent of the behaviour tree entity as the target entity.
    #[default]
    Parent,
    /// Always returns the specified entity as the target entity.
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
    for (bt_entity, mut bt, opt_parent, target_entity) in query.iter_mut() {
        let target_entity = match target_entity {
            BehaveTargetEntity::Parent => {
                opt_parent.map(|p| p.get()).unwrap_or(Entity::PLACEHOLDER)
            }
            BehaveTargetEntity::Entity(e) => *e,
        };
        let tick_result = bt.tick(&time, &mut commands, bt_entity, target_entity);
        match tick_result {
            BehaveNodeStatus::AwaitingTrigger => {
                commands.entity(bt_entity).insert(BehaveAwaitingTrigger);
            }
            BehaveNodeStatus::Success => {
                commands.entity(bt_entity).insert(BehaveFinished(true));
            }
            BehaveNodeStatus::Failure => {
                commands.entity(bt_entity).insert(BehaveFinished(false));
            }
            BehaveNodeStatus::RunningTimer => {}
            BehaveNodeStatus::Running => {}
            BehaveNodeStatus::PendingReset => {}
        }
        if bt.logging && tick_result != BehaveNodeStatus::RunningTimer {
            info!("tick_tree: {bt_entity}\n{}", bt.tree);
        }
    }
}

/// The main behaviour tree component.
/// A `bevy_behave` system will query all entities with a `BehaveTree` to tick them.
/// (unless they have a `BehaveAwaitingTrigger` component)
#[derive(Component)]
#[require(BehaveTargetEntity)]
#[require(Name(||Name::new("BehaveTree")))]
pub struct BehaveTree {
    tree: Tree<BehaveNode>,
    logging: bool,
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

/// Verifies that nodes have the appropriate number of children etc.
fn verify_tree(node: &NodeRef<Behave>) -> bool {
    let children = node.children().collect::<Vec<_>>();
    let n = node.value();
    let range = n.permitted_children();
    if !range.contains(&children.len()) {
        error!(
            "⁉️  Node {n} has {} children! Valid range is: {range:?}",
            children.len(),
        );
        false
    } else {
        for child in children.iter() {
            if !verify_tree(child) {
                return false;
            }
        }
        true
    }
}

impl BehaveTree {
    /// Creates a BehaveTree from an `ego_tree::Tree<BehaveNode>`.
    /// Typically this is created using the behave! macro, but can be
    /// constructed using the ego_tree api too.
    ///
    /// # Panics
    /// An invalid tree will cause a panic here.
    /// Use BehaveTree::verify(&tree) to verify your tree definition first.
    pub fn new(tree: Tree<Behave>) -> Self {
        if !Self::verify(&tree) {
            panic!("Invalid tree");
        }
        // convert to internal BehaveNode tree
        let tree = tree.map(BehaveNode::new);
        Self {
            tree,
            logging: false,
        }
    }

    /// Checks the tree definition is valid by verifying that each node has the correct
    /// number of children.
    pub fn verify(tree: &Tree<Behave>) -> bool {
        verify_tree(&tree.root())
    }

    /// Should verbose logging be enabled? (typically just for debugging).
    pub fn with_logging(mut self, enabled: bool) -> Self {
        self.logging = enabled;
        self
    }

    fn tick(
        &mut self,
        time: &Res<Time>,
        commands: &mut Commands,
        bt_entity: Entity,
        target_entity: Entity,
    ) -> BehaveNodeStatus {
        let mut node = self.tree.root_mut();
        tick_node(
            &mut node,
            time,
            commands,
            bt_entity,
            target_entity,
            self.logging,
        )
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
                if self.logging {
                    debug!(
                        "Setting Dynamic Entity task for {node_id:?} success to {:?}",
                        success
                    );
                }
                *task_status = EntityTaskStatus::Complete(success);
                task_entity
            }
            BehaveNode::TriggerReq { task_status, .. } => {
                if self.logging {
                    debug!(
                        "Setting conditional task for {node_id:?} success to {:?}",
                        success
                    );
                }
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

/// Will report success or failure after a timeout
#[derive(Component, Debug, Clone, Reflect)]
pub struct BehaveTimeout {
    duration: std::time::Duration,
    should_succeed: bool,
    start_time: f32,
}

impl BehaveTimeout {
    /// Creates a new BehaveTimeout which will trigger success or failure after a given duration.
    pub fn new(duration: std::time::Duration, should_succeed: bool) -> Self {
        Self {
            duration,
            should_succeed,
            start_time: 0.0,
        }
    }
    /// Creates a new BehaveTimeout which will trigger success or failure after a given number of seconds
    pub fn from_secs(secs: f32, should_succeed: bool) -> Self {
        Self::new(std::time::Duration::from_secs(secs as u64), should_succeed)
    }
}

fn on_tick_timeout_added(
    t: Trigger<OnAdd, BehaveTimeout>,
    mut q: Query<&mut BehaveTimeout>,
    time: Res<Time>,
) {
    let mut timeout = q.get_mut(t.entity()).unwrap();
    timeout.start_time = time.elapsed_secs();
}

fn tick_timeout_components(
    q: Query<(&BehaveTimeout, &BehaveCtx)>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (timeout, ctx) in q.iter() {
        let elapsed = time.elapsed_secs() - timeout.start_time;
        if elapsed >= timeout.duration.as_secs_f32() {
            if timeout.should_succeed {
                commands.trigger(ctx.success());
            } else {
                commands.trigger(ctx.failure());
            }
        }
    }
}
