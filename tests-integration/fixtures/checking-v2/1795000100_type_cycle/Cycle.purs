module Cycle where

foreign import pair :: forall r1 r2. { x :: { a :: Record r1 | r2 }, y :: { a :: Record (c :: Int | r2), b :: Int | r1 } }

same :: forall a. { x :: a, y :: a } -> Int
same _ = 0

-- Correct: r1 = (c :: Int | r2) and r2 = (b :: Int | r1) has no finite solution: InfiniteType.
test = same pair
