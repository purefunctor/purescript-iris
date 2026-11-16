module R6 where
data Tuple a b = Tuple a b
identity :: forall a. a -> a
identity x = x
-- R5's field used at two types through a non-generalised lambda parameter.
use :: { f :: forall a. a -> a } -> Tuple Int String
use r = Tuple (r.f 1) (r.f "a")
t = (\r -> use r) { f: (identity :: forall a. a -> a) }
