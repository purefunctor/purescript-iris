module GenNested where

data Box a = Box a

class Foo a where
  foo :: a -> Int

-- Foo (Box ?t) has no instance; its argument is not itself an unknown.
test x = foo (Box x)
