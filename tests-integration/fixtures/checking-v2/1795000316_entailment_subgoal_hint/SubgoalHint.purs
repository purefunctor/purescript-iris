module SubgoalHint where

data Box a = Box a

class Shw a where
  shw :: a -> String

instance Shw a => Shw (Box a) where
  shw (Box a) = shw a

-- No Shw Int: the subgoal of Shw (Box Int) fails.
test :: String
test = shw (Box 1)
