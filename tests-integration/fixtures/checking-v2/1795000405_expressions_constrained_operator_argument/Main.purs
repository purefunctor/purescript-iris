module Main where

data Maybe a = Just a | Nothing

foreign import unsafePartial :: forall a. (Partial => a) -> a
foreign import fromJust :: forall a. Partial => Maybe a -> a

apply :: forall a b. (a -> b) -> a -> b
apply f x = f x

infixr 0 apply as $

direct :: forall a. Maybe a -> a
direct m = unsafePartial (fromJust m)

operator :: forall a. Maybe a -> a
operator m = unsafePartial $ fromJust m

compose :: forall a b c. (b -> c) -> (a -> b) -> a -> c
compose f g x = f (g x)

infixr 9 compose as <<<

composed :: forall a b. (Array a -> Maybe b) -> Array a -> b
composed f = unsafePartial $ fromJust <<< f

uncovered :: forall a b. (Array a -> Maybe b) -> Array a -> b
uncovered f = fromJust <<< f
