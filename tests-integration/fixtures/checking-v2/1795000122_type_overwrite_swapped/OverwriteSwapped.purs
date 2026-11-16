module OverwriteSwapped where

foreign import x :: forall r. { a :: Record r | r }

y :: { a :: { b :: Int }, b :: String }
y = { a: { b: 1 }, b: "s" }

-- Same contradiction with the two records swapped. Still the stale-tail path: x = { a: {} } is typed
-- { a :: { b :: Int }, b :: String }.
arr = [y, x]
