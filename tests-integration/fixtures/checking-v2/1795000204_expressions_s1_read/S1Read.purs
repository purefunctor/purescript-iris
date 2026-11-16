module S1Read where
data B = T | F
-- S1 without the case: the solved unknown is the type of a let-free lambda pair.
-- An if in check mode against an unknown that its first branch solves.
t b = if b then (\(r :: { x :: forall a. a -> a }) -> r.x 1) else (\r -> r.x 2)
