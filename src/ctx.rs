use crate::prelude::*;
use bevy::prelude::*;
use ego_tree::NodeId;

pub(crate) fn plugin(app: &mut App) {
    app.add_observer(on_behave_status_report);
}

/// Provided to the user's bevy system or observer fn, so they have a way to report status
/// back to the tree, and to look up the target entity etc.
#[derive(Component, Debug, Copy, Clone)]
pub struct BehaveCtx {
    bt_entity: Entity,
    task_node: NodeId,
    target_entity: Entity,
    sup_entity: Option<Entity>,
    ctx_type: CtxType,
}

impl std::fmt::Display for BehaveCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BehaveCtx(bt: {}, target: {}, type: {:?})",
            self.bt_entity, self.target_entity, self.ctx_type
        )
    }
}

// this is set on BehaveCtx just to catch any errors - we verify when we update the tree that the
// node type matches what we expect. just for peace of mind while developing really.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CtxType {
    Trigger,
    Entity,
}

impl BehaveCtx {
    pub(crate) fn new_for_trigger(task_node: NodeId, tick_ctx: &TickCtx) -> Self {
        Self::new(task_node, tick_ctx, CtxType::Trigger)
    }
    pub(crate) fn new_for_entity(task_node: NodeId, tick_ctx: &TickCtx) -> Self {
        Self::new(task_node, tick_ctx, CtxType::Entity)
    }
    fn new(task_node: NodeId, tick_ctx: &TickCtx, ctx_type: CtxType) -> Self {
        Self {
            task_node,
            bt_entity: tick_ctx.bt_entity,
            target_entity: tick_ctx.target_entity,
            sup_entity: tick_ctx.supervisor_entity,
            ctx_type,
        }
    }
    /// Was this context created for a trigger_req node?
    pub fn is_for_trigger(&self) -> bool {
        self.ctx_type == CtxType::Trigger
    }
    /// Was this context created for a dynamic_spawn node?
    pub fn is_for_entity(&self) -> bool {
        self.ctx_type == CtxType::Entity
    }
    /// Returns the event that reports success for this context.
    pub fn success(&self) -> BehaveStatusReport {
        BehaveStatusReport::Success(*self)
    }
    /// Returns the event that reports failure for this context.
    pub fn failure(&self) -> BehaveStatusReport {
        BehaveStatusReport::Failure(*self)
    }
    /// Returns the target entity for this context.
    /// The target entity is typically the character or game object the behaviour tree is controlling.
    /// See also: [`BehaveTargetEntity`]
    pub fn target_entity(&self) -> Entity {
        self.target_entity
    }
    /// Returns the entity of the behaviour tree that this context is for.
    /// Not typically needed in user code.
    pub fn behave_entity(&self) -> Entity {
        self.bt_entity
    }
    /// Returns the entity of the supervisor that is controlling the behaviour tree.
    /// Only used when running with my unreleased HTN crate that complements bevy_behave.
    pub fn supervisor_entity(&self) -> Option<Entity> {
        self.sup_entity
    }
    /// Returns the node id of the task that this context is for.
    /// Used internally by the tree to report status.
    pub(crate) fn task_node(&self) -> NodeId {
        self.task_node
    }
}

/// Trigger used to signal the completion of a spawn entity task
#[derive(Debug, Event)]
pub enum BehaveStatusReport {
    /// Reports success for a task
    Success(BehaveCtx),
    /// Reports failure for a task
    Failure(BehaveCtx),
}

impl BehaveStatusReport {
    /// Returns the context for this status report.
    pub fn ctx(&self) -> &BehaveCtx {
        match self {
            BehaveStatusReport::Success(ctx) => ctx,
            BehaveStatusReport::Failure(ctx) => ctx,
        }
    }
}

// when we recieve a status report, we add the result to the tree node, so it's processed the
// next time the tree ticks.
fn on_behave_status_report(
    trigger: Trigger<BehaveStatusReport>,
    mut commands: Commands,
    mut q_bt: Query<&mut BehaveTree, Without<BehaveFinished>>,
) {
    let ctx = trigger.event().ctx();
    let Ok(mut bt) = q_bt.get_mut(ctx.behave_entity()) else {
        // This is not necessarily an error - the entity could have been legitimately despawned
        // as part of gameplay logic.
        debug!("Failed to get bt entity during status report {:?}", trigger);
        return;
    };
    // info!(
    //     "📋 Got status report, removing BehaveAwaitingTrigger {:?} node status = {:?}",
    //     trigger.event(),
    //     bt.get_node_result(ctx.task_node())
    // );
    // remove the waiting trigger component, so the tree will be ticked next time.
    commands
        .entity(ctx.bt_entity)
        .remove::<BehaveAwaitingTrigger>();
    let task_entity = match trigger.event() {
        BehaveStatusReport::Success(ctx) => bt.set_node_result(ctx, true),
        BehaveStatusReport::Failure(ctx) => bt.set_node_result(ctx, false),
    };
    // despawn the entity used for this task now that it is complete.
    // if this was a TriggerReq task, there won't be a task entity.
    if let Some(task_entity) = task_entity {
        commands.entity(task_entity).try_despawn_recursive();
    }
}
