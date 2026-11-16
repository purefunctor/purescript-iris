module U8 where
-- Control: an ill-typed argument at an unknown head.
t = \f -> f (if true then 1 else "a")
