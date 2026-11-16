module R2 where
-- Type-changing record update checked against a known result type.
upd :: { a :: Int, b :: Int } -> { a :: String, b :: Int }
upd r = r { a = "x" }
