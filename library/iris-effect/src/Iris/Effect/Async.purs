module Iris.Effect.Async
  ( Async
  , make
  , toStep
  , sync
  , liftEffect
  , service
  , provide
  , fail
  , catch
  , mapError
  , attempt
  , async
  , Fiber
  , fork
  , join
  , await
  , kill
  , bracket
  , finally
  , launch
  ) where

import Prelude

import Data.Either (Either(..))
import Effect (Effect)
import Effect.Exception (throwException)
import Iris.Effect (class Has, getService)
import Iris.Effect.Fiber (Exit(..), Step)
import Iris.Effect.Fiber as Fiber
import Iris.Effect.Sync (Sync)
import Iris.Effect.Sync as Sync

type Fiber error value = Fiber.Fiber error value

newtype Async :: Type -> Row Type -> Type -> Type
newtype Async error services value = Async (Record services -> Step error value)

make ::
  forall error services value. (Record services -> Step error value) -> Async error services value
make = Async

toStep ::
  forall error services value. Record services -> Async error services value -> Step error value
toStep environment (Async program) = program environment

sync :: forall error services value. Sync services value -> Async error services value
sync program = Async \environment -> Fiber.Sync (Sync.run environment program)

liftEffect :: forall error services value. Effect value -> Async error services value
liftEffect effect = Async \_ -> Fiber.Sync effect

service :: forall @service error services. Has service services => Async error services service
service = Async \environment -> Fiber.Pure (getService environment)

provide ::
  forall error inner outer value. Record inner -> Async error inner value -> Async error outer value
provide environment (Async program) = Async \_ -> program environment

fail :: forall error services value. error -> Async error services value
fail error = Async \_ -> Fiber.Throw error

catch ::
  forall inner error services value.
  Async inner services value -> (inner -> Async error services value) -> Async error services value
catch (Async program) handler = Async \environment ->
  Fiber.catchStep (program environment) \error -> toStep environment (handler error)

mapError ::
  forall inner error services value.
  (inner -> error) -> Async inner services value -> Async error services value
mapError function program = catch program (fail <<< function)

attempt ::
  forall inner error services value.
  Async inner services value -> Async error services (Either inner value)
attempt program = catch (map Right program) (pure <<< Left)

async ::
  forall error services value.
  ((Either error value -> Effect Unit) -> Effect (Async error services Unit)) ->
  Async error services value
async register = Async \environment -> Fiber.Async \callback -> do
  Async canceler <- register callback
  pure (canceler environment)

fork ::
  forall error other services value.
  Async error services value -> Async other services (Fiber error value)
fork (Async program) = Async \environment -> Fiber.forkStep (program environment) identity

join :: forall error services value. Fiber error value -> Async error services value
join fiber = Async \_ -> Fiber.bindStep (awaitStep fiber) case _ of
  Succeeded value -> Fiber.Pure value
  Failed error -> Fiber.Throw error
  Died defect -> Fiber.Sync (throwException defect)

await ::
  forall error other services value. Fiber error value -> Async other services (Exit error value)
await fiber = Async \_ -> awaitStep fiber

awaitStep :: forall error other value. Fiber error value -> Step other (Exit error value)
awaitStep fiber = Fiber.Async \callback -> do
  cancel <- Fiber.joinFiber fiber \exit -> callback (Right exit)
  pure (Fiber.Sync cancel)

kill :: forall error services value. error -> Fiber error value -> Async error services Unit
kill error fiber = Async \_ -> Fiber.Async \callback -> do
  cancel <- Fiber.killFiber error fiber (callback (Right unit))
  pure (Fiber.Sync cancel)

bracket ::
  forall error services resource value.
  Async error services resource ->
  (resource -> Async error services Unit) ->
  (resource -> Async error services value) ->
  Async error services value
bracket acquire release use = Async \environment ->
  let
    releaseStep resource = toStep environment (release resource)
    conditions =
      { killed: releaseStep
      , failed: \_ -> releaseStep
      , died: \_ -> releaseStep
      , completed: \_ -> releaseStep
      }
  in
    Fiber.bracketStep (toStep environment acquire) conditions (toStep environment <<< use)

finally ::
  forall error services value.
  Async error services Unit -> Async error services value -> Async error services value
finally release program = bracket (pure unit) (\_ -> release) (\_ -> program)

launch ::
  forall error services value.
  { onUncaught :: error -> Effect Unit } ->
  Record services ->
  Async error services value ->
  Effect (Fiber error value)
launch { onUncaught } environment (Async program) = do
  runtime <- Fiber.newRuntime
  fiber <- Fiber.makeFiber runtime (program environment)
  _ <- Fiber.joinFiber fiber case _ of
    Succeeded _ -> pure unit
    Failed error -> onUncaught error
    Died defect -> throwException defect
  pure fiber

deferred ::
  forall error services value. Record services -> Async error services value -> Step error value
deferred environment (Async program) = Fiber.deferStep \_ -> program environment

instance functorAsync :: Functor (Async error services) where
  map function program = Async \environment -> Fiber.mapStep function (deferred environment program)

instance applyAsync :: Apply (Async error services) where
  apply function argument = Async \environment ->
    Fiber.bindStep (deferred environment function) \apply ->
      Fiber.mapStep apply (deferred environment argument)

instance applicativeAsync :: Applicative (Async error services) where
  pure value = Async \_ -> Fiber.Pure value

instance bindAsync :: Bind (Async error services) where
  bind program continuation = Async \environment ->
    Fiber.bindStep (deferred environment program) \value -> toStep environment (continuation value)

instance monadAsync :: Monad (Async error services)
