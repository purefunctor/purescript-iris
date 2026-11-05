module Iris.Effect.Sync
  ( Sync
  , make
  , run
  , service
  , liftEffect
  , provide
  ) where

import Prelude

import Effect (Effect)
import Iris.Effect (class Has, getService)

newtype Sync :: Row Type -> Type -> Type
newtype Sync services value = Sync (Record services -> Effect value)

make :: forall services value. (Record services -> Effect value) -> Sync services value
make = Sync

run :: forall services value. Record services -> Sync services value -> Effect value
run environment (Sync program) = program environment

service :: forall @service services. Has service services => Sync services service
service = Sync \environment -> pure (getService environment)

liftEffect :: forall services value. Effect value -> Sync services value
liftEffect effect = Sync \_ -> effect

provide :: forall inner outer value. Record inner -> Sync inner value -> Sync outer value
provide environment (Sync program) = Sync \_ -> program environment

instance functorSync :: Functor (Sync services) where
  map function (Sync program) = Sync \environment -> map function (program environment)

instance applySync :: Apply (Sync services) where
  apply (Sync function) (Sync argument) =
    Sync \environment -> apply (function environment) (argument environment)

instance applicativeSync :: Applicative (Sync services) where
  pure value = Sync \_ -> pure value

instance bindSync :: Bind (Sync services) where
  bind (Sync program) continuation =
    Sync \environment -> program environment >>= \value -> run environment (continuation value)

instance monadSync :: Monad (Sync services)
