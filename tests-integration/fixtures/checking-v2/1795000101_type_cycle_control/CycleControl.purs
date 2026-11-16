module CycleControl where

foreign import pair :: forall r. { x :: { | r }, y :: { b :: Int | r } }

same :: forall a. { x :: a, y :: a } -> Int
same _ = 0

test = same pair
