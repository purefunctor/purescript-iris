module U4 where
-- The argument mentions the head: f (f { x: 1 }).
t = \f -> f (f { x: 1 })
