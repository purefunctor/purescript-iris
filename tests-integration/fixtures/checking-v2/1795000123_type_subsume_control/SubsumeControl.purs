module SubsumeControl where

foreign import x :: forall r. { a :: Record r | r }

-- Same contradiction as Overwrite, reached through subsumption instead of unifyRows.
z :: { a :: { b :: Int }, b :: String }
z = x
