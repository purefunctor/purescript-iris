module Erase where

foreign import pair :: forall r. { x :: { a :: Record r | r }, y :: { a :: { b :: Int } | r } }

pick :: forall a. { x :: a, y :: a } -> a
pick p = p.y

-- Correct: r = (b :: Int), so test :: { a :: { b :: Int }, b :: Int }.
test = pick pair

-- If the answer for r is lost, test generalizes to forall r. { a :: Record r | r }.
claim :: { a :: { c :: String }, c :: String }
claim = test
