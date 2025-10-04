use bevy::{log::LogPlugin, prelude::*};
use bevy_behave::prelude::*;

/// ask BehaveTree to log transitions. verbose with logs of enemies!
const ENABLE_LOGGING: bool = true;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(LogPlugin::default());
    app.add_plugins(BehavePlugin::default());
    app.add_systems(Startup, init);
    app.add_observer(on_my_condition);
    app.add_observer(on_my_action);
    app.add_systems(
        Update,
        delayed_report.run_if(resource_exists::<DelayedReporter>),
    );
    app.run();
}

#[derive(Resource)]
struct DelayedReporter(f32, BehaveCtx);

fn delayed_report(dr: Res<DelayedReporter>, mut commands: Commands, time: Res<Time>) {
    if time.elapsed_secs() > dr.0 {
        info!("Delayed.. now reporting success");
        commands.trigger(dr.1.success());
        commands.remove_resource::<DelayedReporter>();
    }
}

#[derive(Clone)]
struct MyCondition;

#[derive(Clone)]
struct MyAction;

fn on_my_condition(
    trigger: On<BehaveTrigger<MyCondition>>,
    mut commands: Commands,
    mut counter: Local<u32>,
) {
    *counter += 1;
    info!("MyCondition: {}", *counter);
    let ctx = trigger.event().ctx();
    if *counter < 5 {
        commands.trigger(ctx.success());
    } else {
        commands.trigger(ctx.failure());
    }
}

fn on_my_action(trigger: On<BehaveTrigger<MyAction>>, mut commands: Commands, time: Res<Time>) {
    info!("MyAction!");
    // commands.trigger(trigger.event().ctx().success());
    // let's pretend we're doing something clever, and reporting success after a delay:
    commands.insert_resource(DelayedReporter(
        time.elapsed_secs() + 1.0,
        *trigger.event().ctx(),
    ));
}

fn init(mut commands: Commands) {
    let tree = tree! {
        Behave::While => {
            Behave::trigger(MyCondition),
            Behave::Sequence => {
                Behave::trigger(MyAction),
                Behave::Wait(1.0),
            }
        }
    };
    let target = commands.spawn(()).id();
    commands.spawn((
        Name::new("Behave tree"),
        BehaveTree::new(tree).with_logging(ENABLE_LOGGING),
        BehaveTargetEntity::Entity(target),
    ));
}
