module Main where

import Prelude

lookup :: forall m. Monad m => m { value :: Int } -> (Int -> m Unit) -> m Int
lookup ask record = do
  { value } <- ask
  record value
  pure value
