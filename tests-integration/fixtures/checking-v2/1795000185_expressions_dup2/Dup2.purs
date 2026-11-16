module Dup2 where
-- Duplicate labels with different types: which one does r.a read?
t :: Int
t = { a: 1, a: "s" }.a
