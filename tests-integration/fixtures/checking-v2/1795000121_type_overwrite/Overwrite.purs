module Overwrite where

foreign import x :: forall r. { a :: Record r | r }

y :: { a :: { b :: Int }, b :: String }
y = { a: { b: 1 }, b: "s" }

-- Correct: r = (b :: Int) from field a, but the tail says r = (b :: String): a type error.
arr = [x, y]

-- If this compiles, y.a.b (an Int at runtime) is typed as String.
claim :: Array { a :: { b :: String }, b :: String }
claim = arr
