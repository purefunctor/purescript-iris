module R7 where
-- Record update whose object has an unknown type, checked against a closed record.
t :: { a :: Int } -> { a :: String }
t = \r -> r { a = "x" }
