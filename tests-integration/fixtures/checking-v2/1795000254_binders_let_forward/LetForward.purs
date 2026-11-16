module LetForward where
data P = P Int Int
foreign import mk :: P
-- A let value that refers to a name bound by a later pattern binding.
t :: Int
t =
  let g = x
      P x _ = mk
  in g
