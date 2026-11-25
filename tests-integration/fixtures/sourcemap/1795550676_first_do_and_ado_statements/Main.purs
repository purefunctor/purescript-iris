module Main where

import Prelude

firstBind :: forall m. Monad m => m Int -> (Int -> m Unit) -> m Int
firstBind ask record = do
  value <- ask
  record value
  pure value

firstDiscard :: forall m. Monad m => m Unit -> m Int -> m Int
firstDiscard announce ask = do
  announce
  ask

firstAdoAction :: forall f. Apply f => f Int -> f Int -> f Int
firstAdoAction left right = ado
  a <- left
  b <- right
  in a + b
