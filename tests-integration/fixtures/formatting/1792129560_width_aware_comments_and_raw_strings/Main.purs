-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

comments = [ firstElement -- trailing before the comma
           , {- inline -} secondElement
           -- standalone before the final item
           , thirdElement ]
multiline = consume """first

  preserve this indentation


last
""" {- comment

    preserve this too

-} argument
