use bevy::{
    prelude::*,
    reflect::serde::{ReflectDeserializer, ReflectSerializer},
    scene::ron,
};
use bevy_behave::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    app.add_systems(Startup, insert_bt);
    app.add_plugins(BehavePlugin::default());
    app.add_plugins(slow_action_plugin);
    app.add_observer(on_tree_finished);
    // app.register_type::<MyCond>();
    app.add_observer(on_my_cond);
    app.run();
}

fn on_tree_finished(
    trigger: Trigger<OnAdd, BehaveFinished>,
    q: Query<(&BehaveFinished, &BehaveTree)>,
) {
    let (result, tree) = q.get(trigger.entity()).unwrap();
    info!("Tree finished: {result:?} tree = \n{tree}");
}

#[derive(Debug, Reflect, Clone, Event)]
pub struct MyTest {
    foo: f32,
}

fn on_my_cond(trigger: Trigger<BehaveTrigger<MyTest>>, mut commands: Commands) {
    let ev = trigger.event();
    info!("TRIG {ev:?}");
    let ctx = trigger.event().ctx();
    let response = ctx.success();
    info!("Triggering response: {response:?}");
    commands.trigger(response);
}

#[derive(Event, Copy, Clone)]
pub struct MyCond {
    foo: f32,
}

fn insert_bt(mut commands: Commands) {
    let parent = commands.spawn(Name::new("parent")).id();
    info!("Parent : {parent}");
    let t = tree! {
        Behave::Fallback => {
            Behave::dynamic_spawn((
                SlowAction::failing("Single Slowcoach", 2.0),
                Name::new("Single Slowcoach failing")
            )),
            Behave::AlwaysFail,
            Behave::Invert => {
                Behave::dynamic_spawn((
                    SlowAction::succeeding("Single Slowcoach inside invert", 1.0),
                    Name::new("Single Slowcoach inside invert")
                )),
            },
            Behave::Invert => {
                Behave::trigger_req(MyTest { foo: 3.1 }),
            },
            Behave::dynamic_spawn((
                SlowAction::succeeding("Single Slowcoach", 1.0),
                Name::new("Single Slowcoach")
            )),
            Behave::AlwaysSucceed,
        }
    };

    let bt = BehaveTree::new(t);
    let bt_ent = commands
        .spawn((Name::new("bt entity"), bt))
        .set_parent(parent)
        .id();
    warn!("BT ENT {bt_ent}");
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
    app.add_systems(Update, slow_action_system);
    // .run_if(on_timer(Duration::from_secs(1))),
    // );
    app.add_observer(on_slow_action_added);
}

fn on_slow_action_added(trigger: Trigger<OnAdd, SlowAction>, q: Query<(&SlowAction, &BehaveCtx)>) {
    let slow = q.get(trigger.entity()).unwrap();
    info!("Slow action added: {:?} {:?}", trigger.entity(), slow);
}

fn slow_action_system(
    mut q: Query<(&BehaveCtx, &mut SlowAction)>,
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
