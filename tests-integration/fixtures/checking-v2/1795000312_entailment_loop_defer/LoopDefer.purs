module LoopDefer where

data Box a = Box a

class D a b | a -> b
instance D a (Box a)

class C a
-- C's subgoal C b is listed before D a b, so C ?b is deferred (b unknown) and
-- re-visited after D a b improves ?b := Box a.
instance (C b, D a b) => C (Box a)

g :: forall x. C x => x -> Int
g _ = 0

f :: Int
f = g (Box 1)
