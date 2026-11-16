module LoopEager where

data Box a = Box a

class D a b | a -> b
instance D a (Box a)

class C a
-- Same as LoopDefer, but D a b is solved first, so C ?b is solved at work + 1.
instance (D a b, C b) => C (Box a)

g :: forall x. C x => x -> Int
g _ = 0

f :: Int
f = g (Box 1)
