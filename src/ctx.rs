use crate::prelude::*;
use bevy::prelude::*;
use ego_tree::NodeId;

pub(crate) fn plugin(app: &mut App) {
    app.add_observer(on_behave_status_report);
}

/// Provided to the user's bevy system or observer code, so they have a way to report status
/// back to the tree, and to look up the target entity etc.
#[derive(Component, Debug, Copy, Clone)]
pub struct BehaveCtx {
    bt_entity: Entity,
    task_node: NodeId,
    target_entity: Entity,
    ctx_type: CtxType,
}

// this is set on BehaveCtx just to catch any errors - we verify when we update the tree that the
// node type matches what we expect. just for peace of mind while developing really.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CtxType {
    Trigger,
    Entity,
}

impl BehaveCtx {
    pub(crate) fn new_for_trigger(
        bt_entity: Entity,
        task_node: NodeId,
        target_entity: Entity,
    ) -> Self {
        Self::new(bt_entity, task_node, target_entity, CtxType::Trigger)
    }
    pub(crate) fn new_for_entity(
        bt_entity: Entity,
        task_node: NodeId,
        target_entity: Entity,
    ) -> Self {
        Self::new(bt_entity, task_node, target_entity, CtxType::Entity)
    }
    fn new(bt_entity: Entity, task_node: NodeId, target_entity: Entity, ctx_type: CtxType) -> Self {
        Self {
            bt_entity,
            task_node,
            target_entity,
            ctx_type,
        }
    }
    pub fn is_for_trigger(&self) -> bool {
        self.ctx_type == CtxType::Trigger
    }
    pub fn is_for_entity(&self) -> bool {
        self.ctx_type == CtxType::Entity
    }
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
    pub(crate) fn task_node(&self) -> NodeId {
        self.task_node
    }
}

/// Trigger used to signal the completion of a spawn entity task
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

// when we recieve a status report, we add the result to the tree node, so it's processed the
// next time the tree ticks.
fn on_behave_status_report(
    trigger: Trigger<BehaveStatusReport>,
    mut commands: Commands,
    mut q_bt: Query<&mut BehaveTree, Without<BehaveFinished>>,
) {
    // info!("Got status report: {:?}", trigger);
    let ctx = trigger.event().ctx();
    let Ok(mut bt) = q_bt.get_mut(ctx.behave_entity()) else {
        error!("Failed to get bt entity {:?}", trigger);
        return;
    };
    // info!("📋 Got status report: {:?}", trigger.event());
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
