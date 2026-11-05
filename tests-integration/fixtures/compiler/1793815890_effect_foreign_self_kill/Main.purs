module Main (insideCatch, beforeImmediateCallback) where

import Prelude

import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import Effect (Effect)
import Effect.Ref as Ref
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async
import Iris.Effect.Fiber as Fiber

type Report = String -> Effect Unit

foreign import later :: Effect Unit -> Effect Unit

say :: forall services. Report -> String -> Async String services Unit
say report message = Async.liftEffect (report message)

killSelf :: forall services. Async.Fiber String Unit -> Async String services Unit
killSelf fiber = Async.liftEffect (void (Fiber.killFiber "self" fiber (pure unit)))

withSelf :: Report -> (Async.Fiber String Unit -> Async String () Unit) -> Effect Unit
withSelf report body = void do
  Async.launch { onUncaught: \failure -> report ("uncaught " <> failure) } {} do
    self <- Async.liftEffect (Ref.new Nothing)
    fiber <- Async.fork do
      Async.async \callback -> do
        later (callback (Right unit))
        pure (pure unit)
      found <- Async.liftEffect (Ref.read self)
      case found of
        Just me -> body me
        Nothing -> say report "missing fiber"
    Async.liftEffect (Ref.write (Just fiber) self)
    exit <- Async.await fiber
    say report case exit of
      Fiber.Succeeded _ -> "succeeded"
      Fiber.Failed failure -> "failed " <> failure
      Fiber.Died _ -> "died"

insideCatch :: Report -> Effect Unit
insideCatch report = withSelf report \me -> do
  Async.catch
    ( do
        say report "before kill"
        killSelf me
        say report "after kill"
    )
    \failure -> say report ("caught " <> failure)
  say report "after catch"

beforeImmediateCallback :: Report -> Effect Unit
beforeImmediateCallback report = withSelf report \me -> do
  say report "before kill"
  killSelf me
  value <- Async.async \callback -> do
    callback (Right 5)
    pure (pure unit)
  say report ("after callback " <> show (value :: Int))
