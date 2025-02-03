use std::{any::Any, marker::PhantomData, time::Duration};
mod dyn_bundle;
use bevy::{core_pipeline::experimental::taa, prelude::*, time::common_conditions::on_timer};
use dyn_bundle::prelude::*;
use ego_tree::*;
mod bt;
use bt::*;

#[derive(Component, Clone)]
struct CompA;

#[derive(Component, Clone)]
struct CompB(f32);

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_systems(Startup, insert_bt);
    app.add_plugins(bt::bt_plugin);
    app.add_plugins(slow_action_plugin);
    app.run();
}

fn insert_bt(mut commands: Commands) {
    let t = tree!(Behaviour::SequenceFlow(vec![
        Behaviour::Wait(1.0),
        Behaviour::SpawnTask(DynamicBundel::new((
            SlowAction::succeeding("Single Slowcoach", 1.0),
            Name::new("Single Slowcoach")
        ))),
    ]));

    let bt = BehaviourTree::new(t);

    commands.spawn((Name::new("bt entity"), bt));
}

fn dump(q: Query<Entity>, mut commands: Commands) {
    for e in q.iter() {
        info!("Entity: {:?}", e);
        commands.entity(e).log_components();
    }
}

#[derive(Component, Debug, Clone)]
struct SlowAction {
    name: String,
    start: Option<f32>,
    dur: f32,
    will_succeed: bool,
}

impl SlowAction {
    pub fn succeeding(name: impl Into<String>, dur: f32) -> Self {
        Self {
            name: name.into(),
            dur,
            will_succeed: true,
            start: None,
        }
    }
    pub fn failing(name: impl Into<String>, dur: f32) -> Self {
        Self {
            name: name.into(),
            dur,
            will_succeed: false,
            start: None,
        }
    }
}

fn slow_action_plugin(app: &mut App) {
    app.add_systems(
        Update,
        slow_action_system.run_if(on_timer(Duration::from_secs(1))),
    );
    app.add_observer(on_slow_action_added);
}

fn on_slow_action_added(
    trigger: Trigger<OnAdd, SlowAction>,
    mut commands: Commands,
    q: Query<&SlowAction>,
) {
    let slow = q.get(trigger.entity()).unwrap();
    info!("Slow action added: {:?} {:?}", trigger.entity(), slow);
}

fn slow_action_system(
    mut q: Query<(&BtCtx, &mut SlowAction)>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (ctx, mut slow) in q.iter_mut() {
        if let Some(start) = slow.start {
            let elapsed = time.elapsed_secs() - start;
            if elapsed > slow.dur {
                if slow.will_succeed {
                    info!("Slow action succeeded: {:?}", slow.name);
                    commands.trigger(ctx.success());
                } else {
                    info!("Slow action failed: {:?}", slow.name);
                    commands.trigger(ctx.failure());
                }
            }
        } else {
            slow.start = Some(time.elapsed_secs());
        }
    }
}
