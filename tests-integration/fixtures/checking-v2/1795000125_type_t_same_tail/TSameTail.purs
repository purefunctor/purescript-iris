module TSameTail where

-- Two identical open rows at the type level: must be accepted, and r stays open.
foreign import p :: forall r. { x :: { a :: Int | r }, y :: { a :: Int | r } }

pick :: forall a. { x :: a, y :: a } -> a
pick q = q.y

test = pick p
