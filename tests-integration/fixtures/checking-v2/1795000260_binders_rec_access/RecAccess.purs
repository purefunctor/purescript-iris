module RecAccess where
foreign import r :: { a :: Int, a :: String }
foreign import neg :: Int -> Int
-- Control: the accessor r.a has the first label's type.
result :: Int
result = neg r.a
