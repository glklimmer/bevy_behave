use crate::prelude::BehaveCtx;
use bevy::prelude::*;

/// A wrapper around a user-provided type, which we trigger to test a condition.
#[derive(Event, Debug, Clone)]
pub struct BehaveTrigger<T: Clone + Send + Sync> {
    pub(crate) inner: T,
    pub(crate) ctx: BehaveCtx,
}

impl<T: Clone + Send + Sync> BehaveTrigger<T> {
    pub fn ctx(&self) -> &BehaveCtx {
        &self.ctx
    }
    pub fn inner(&self) -> &T {
        &self.inner
    }
}

pub trait BehaveUserTrigger: Send + Sync {
    fn trigger(&self, commands: &mut Commands, ctx: BehaveCtx);
    fn clone_box(&self) -> Box<dyn BehaveUserTrigger>;
}

impl<T: Clone + Send + Sync + 'static> BehaveUserTrigger for T {
    fn trigger(&self, commands: &mut Commands, ctx: BehaveCtx) {
        commands.trigger(BehaveTrigger::<T> {
            inner: self.clone(),
            ctx,
        });
    }

    fn clone_box(&self) -> Box<dyn BehaveUserTrigger> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn BehaveUserTrigger> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}
