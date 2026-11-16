module U9 where
identity :: forall a. a -> a
identity x = x
foreign import g :: (forall a. a -> a) -> Int
-- f's argument type ?t -> ?t is fixed outside g's forall, so g needs ?t := a skolem.
t = (\f -> f identity) g
