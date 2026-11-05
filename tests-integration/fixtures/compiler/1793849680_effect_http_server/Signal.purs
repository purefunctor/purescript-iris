module Signal (Signal, new, fire, wait) where

import Prelude

import Data.Array as Array
import Data.Either (Either(..))
import Data.Foldable (sequence_)
import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

newtype Signal = Signal (Ref (Either (Array (Effect Unit)) Unit))

new :: Effect Signal
new = Signal <$> Ref.new (Left [])

fire :: Signal -> Effect Unit
fire (Signal state) = do
  current <- Ref.read state
  case current of
    Left waiters -> do
      Ref.write (Right unit) state
      sequence_ waiters
    Right _ -> pure unit

wait :: forall error services. Signal -> Async error services Unit
wait (Signal state) = Async.async \callback -> do
  current <- Ref.read state
  case current of
    Left waiters -> Ref.write (Left (Array.snoc waiters (callback (Right unit)))) state
    Right _ -> callback (Right unit)
  pure (pure unit)
