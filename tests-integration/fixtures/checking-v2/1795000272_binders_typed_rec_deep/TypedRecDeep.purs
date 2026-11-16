module TypedRecDeep where
-- OQ-5.1: the annotation is an instance of the argument type, one level down.
-- A subtype(expected, annotation) rule accepts it; purs's Unify rejects it.
h :: { x :: forall a. a -> a } -> Int
h = \(r :: { x :: Int -> Int }) -> r.x 1
