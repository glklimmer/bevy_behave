use bevy::{log::LogPlugin, prelude::*};
use bevy_behave::prelude::*;

/// ask BehaveTree to log transitions
const ENABLE_LOGGING: bool = true;

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(LogPlugin::default());
    app.add_plugins(BehavePlugin::default());
    app.add_systems(Startup, init);
    app.add_observer(on_my_if_condition);
    app.add_observer(on_my_then_action);
    app.add_observer(on_my_else_action);
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
struct MyIfCondition;

#[derive(Clone)]
struct MyThenAction;

#[derive(Clone)]
struct MyElseAction;

fn on_my_if_condition(
    trigger: Trigger<BehaveTrigger<MyIfCondition>>,
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

fn on_my_then_action(
    trigger: Trigger<BehaveTrigger<MyThenAction>>,
    mut commands: Commands,
    time: Res<Time>,
) {
    info!("MyAction!");
    // commands.trigger(trigger.event().ctx().success());
    // let's pretend we're doing something clever, and reporting success after a delay:
    commands.insert_resource(DelayedReporter(
        time.elapsed_secs() + 1.0,
        *trigger.event().ctx(),
    ));
}

// contrived example. this would be better off just being `Behave::AlwaysFail`.
fn on_my_else_action(trigger: Trigger<BehaveTrigger<MyElseAction>>, mut commands: Commands) {
    info!("MyElseAction!");
    commands.trigger(trigger.event().ctx().failure());
}

fn init(mut commands: Commands) {
    // slightly contrived example, becase an IfThen without the third else child will return the
    // result of the second child, or failure if the first child fails, so the else child isn't
    // strictly needed here – a failure of the MyIfCondition would break us out of the loop.
    let tree = tree! {
        Behave::While => {
            Behave::IfThen => {
                Behave::trigger(MyIfCondition),
                Behave::Sequence => {
                    Behave::trigger(MyThenAction),
                    Behave::Wait(1.0),
                },
                Behave::trigger(MyElseAction),
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
