module Main where

-- Each synonym doubles the row, so `Fields13 ()` expands to a chain of
-- 8192 row tails that must be flattened without recursing per tail.
type Fields0 r = ( field :: Int | r )
type Fields1 r = Fields0 (Fields0 r)
type Fields2 r = Fields1 (Fields1 r)
type Fields3 r = Fields2 (Fields2 r)
type Fields4 r = Fields3 (Fields3 r)
type Fields5 r = Fields4 (Fields4 r)
type Fields6 r = Fields5 (Fields5 r)
type Fields7 r = Fields6 (Fields6 r)
type Fields8 r = Fields7 (Fields7 r)
type Fields9 r = Fields8 (Fields8 r)
type Fields10 r = Fields9 (Fields9 r)
type Fields11 r = Fields10 (Fields10 r)
type Fields12 r = Fields11 (Fields11 r)
type Fields13 r = Fields12 (Fields12 r)

test :: Record (Fields13 ()) -> Int
test record = record.field
