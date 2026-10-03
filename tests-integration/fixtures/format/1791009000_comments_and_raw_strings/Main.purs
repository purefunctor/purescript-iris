module Main where
{- outer {- nested -} block -}
raw="""first line
  second "quoted" line
last line"""
-- Keep this attached to the declaration.
value=raw -- trailing comment
-- comment at EOF
