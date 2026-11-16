module DeterminedPosition where

data Tuple a b = Tuple a b

data State s a = State s a

class Get s m | m -> s where
  get :: m s

instance Get s (State s) where
  get = get

first :: forall m a. Get (Tuple Int a) m => m Int -> Int
first _ = 0

improved :: forall s. State (Tuple Int s) Int -> Int
improved = first

mismatched :: State Int Int -> Int
mismatched = first
