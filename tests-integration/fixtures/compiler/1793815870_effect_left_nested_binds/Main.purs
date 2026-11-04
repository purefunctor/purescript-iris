module Main (run) where

import Prelude

import Data.Array as Array
import Effect (Effect)
import Effect.Ref as Ref
import Iris.Effect.Async (Async)
import Iris.Effect.Async as Async

counted :: Async String () Int
counted =
  Array.foldl (\action _ -> action >>= \value -> pure (value + 1)) (pure 0) (Array.range 1 100000)

sequenced :: Ref.Ref Int -> Async String () Int
sequenced effects =
  Array.foldl (\action _ -> action *> Async.liftEffect (Ref.modify (_ + 1) effects)) (pure 0)
    (Array.range 1 100000)

mapped :: Async String () Int
mapped = Array.foldl (\action _ -> map (_ + 1) action) (pure 0) (Array.range 1 100000)

applied :: Async String () Int
applied = Array.foldl (\action _ -> (+) <$> action <*> pure 1) (pure 0) (Array.range 1 100000)

run :: (String -> Effect Unit) -> Effect Unit
run report = void do
  Async.launch { onUncaught: \failure -> report ("uncaught " <> failure) } {} do
    total <- counted
    Async.liftEffect (report ("total " <> show total))
    effects <- Async.liftEffect (Ref.new 0)
    last <- sequenced effects
    performed <- Async.liftEffect (Ref.read effects)
    Async.liftEffect (report ("effects " <> show last <> " " <> show performed))
    mappedTotal <- mapped
    Async.liftEffect (report ("mapped " <> show mappedTotal))
    appliedTotal <- applied
    Async.liftEffect (report ("applied " <> show appliedTotal))
