module LetForwardName where
-- Control for LetForward: the later binding is a plain name.
t :: Int
t =
  let g = x
      x = 1
  in g
