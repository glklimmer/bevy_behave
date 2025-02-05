// This is a based on https://crates.io/crates/bevy_dynamic_bundle
// and updated for newer bevy version, with some bevy_behave specific additions.
use bevy::ecs::system::{EntityCommand, EntityCommands};
use bevy::prelude::{Bundle, Commands, Entity, World};

use dyn_clone::DynClone;

use crate::ctx::BehaveCtx;

pub mod prelude {
    pub use super::{DynamicBundel, DynamicInsert, DynamicSpawn};
}

/// we want to insert the BehaveCtx at the same time as the dynamic bundle, because we want to be
/// able to write Trigger<OnAdd, BehaveCtx> and see the bundle components on the entity already.
fn insert<T: Bundle + Clone>(bundle: T) -> impl DynEntityCommand {
    move |entity: Entity, world: &mut World, ctx: Option<BehaveCtx>| {
        if let Ok(mut entity) = world.get_entity_mut(entity) {
            if let Some(ctx) = ctx {
                entity.insert((ctx, bundle));
            } else {
                entity.insert(bundle);
            }
        } else {
            panic!(
                "error[B0003]: Could not insert a bundle (of type `{}`) for entity {:?} because it doesn't exist in this World.",
                std::any::type_name::<T>(),
                entity
            );
        }
    }
}

trait DynEntityCommand<Marker = ()>: DynClone + Send + Sync + 'static {
    fn apply_dyn_bundle(self: Box<Self>, id: Entity, world: &mut World, ctx: Option<BehaveCtx>);
}

impl<F> DynEntityCommand for F
where
    F: FnOnce(Entity, &mut World, Option<BehaveCtx>) + DynClone + Send + Sync + 'static,
{
    fn apply_dyn_bundle(self: Box<Self>, id: Entity, world: &mut World, ctx: Option<BehaveCtx>) {
        self(id, world, ctx);
    }
}

impl EntityCommand for DynamicSpawnWrapper {
    fn apply(self, id: Entity, world: &mut World) {
        self.bundel_fn.apply_dyn_bundle(id, world, self.ctx);
    }
}

dyn_clone::clone_trait_object!(DynEntityCommand);
#[derive(Clone)]
pub struct DynamicBundel {
    #[allow(dead_code)]
    bundle_fn: Box<dyn DynEntityCommand>,
}
impl std::fmt::Debug for DynamicBundel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DynamicBundle")
    }
}

impl DynamicBundel {
    pub fn new<T: Bundle + Clone>(bundle: T) -> DynamicBundel {
        DynamicBundel {
            bundle_fn: Box::new(insert(bundle)),
        }
    }
}

impl<T: Bundle + Clone> From<T> for DynamicBundel {
    fn from(bundle: T) -> Self {
        DynamicBundel::new(bundle)
    }
}

#[allow(dead_code)]
pub trait DynamicInsert<'a> {
    fn dyn_insert(
        &mut self,
        dyn_bundel: DynamicBundel,
        ctx: Option<BehaveCtx>,
    ) -> &mut EntityCommands<'a>;
}

impl<'a> DynamicInsert<'a> for EntityCommands<'a> {
    fn dyn_insert(
        &mut self,
        dyn_bundel: DynamicBundel,
        ctx: Option<BehaveCtx>,
    ) -> &mut EntityCommands<'a> {
        self.queue(DynamicSpawnWrapper {
            bundel_fn: dyn_bundel.bundle_fn,
            ctx,
        });
        self
    }
}

struct DynamicSpawnWrapper {
    bundel_fn: Box<dyn DynEntityCommand>,
    ctx: Option<BehaveCtx>,
}

#[allow(dead_code)]
pub trait DynamicSpawn {
    fn dyn_spawn(&mut self, dyn_bundel: DynamicBundel, ctx: Option<BehaveCtx>) -> EntityCommands;
}

// Implementation for Commands
impl DynamicSpawn for Commands<'_, '_> {
    fn dyn_spawn(&mut self, dyn_bundel: DynamicBundel, ctx: Option<BehaveCtx>) -> EntityCommands {
        let mut entity_commands = self.spawn(());
        entity_commands.dyn_insert(dyn_bundel, ctx);
        entity_commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;

    #[test]
    fn simple_dyn_bundle_test() {
        #[derive(Component, Clone)]
        struct ComponentA(i32);

        App::new()
            .add_systems(Startup, (setup, query).chain())
            .run();

        fn setup(mut commands: Commands) {
            let dyn_bundle = DynamicBundel::new(ComponentA(2));

            //commands.spawn(()).dyn_insert(dyn_bundle.clone());
            commands.dyn_spawn(dyn_bundle, None);
        }

        fn query(components: Query<&ComponentA>) {
            assert_eq!(2, components.get_single().unwrap().0);
        }
    }

    #[test]
    fn spawner_test() {
        #[derive(Component, Clone)]
        struct Spawner(DynamicBundel);

        #[derive(Component, Clone)]
        struct ComponentA(i32);

        App::new()
            .add_systems(Startup, (setup, spawn, query).chain())
            .run();

        fn setup(mut commands: Commands) {
            let dyn_bundle = DynamicBundel::new(ComponentA(2));

            //commands.spawn(()).dyn_insert(dyn_bundle.clone());
            commands.spawn(Spawner(dyn_bundle));
        }

        fn spawn(mut commands: Commands, spawner_q: Query<&Spawner>) {
            let spawner = spawner_q.get_single().unwrap();
            commands.dyn_spawn(spawner.0.clone(), None);
        }

        fn query(components: Query<&ComponentA>) {
            assert_eq!(2, components.get_single().unwrap().0);
        }
    }
}
