module RecMixedLeakUse where
foreign import k :: forall a b. a -> b -> a
f :: forall a. a -> a
f x = g x
g x = k x f
-- If g escaped as a0 -> a0, a use at Int is where it bites.
use :: Int
use = g 1
