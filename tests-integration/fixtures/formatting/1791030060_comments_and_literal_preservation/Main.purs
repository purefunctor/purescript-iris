-- | Keep documentation before its module.
module   Main where

-- | Keep this comment with the declaration.
value=1  -- trailing comment

nested =do
      pure 1 {- outer {- inner -} outer -}
      -- between statements
      pure 2

raw="""first
      indented raw text
last"""
unicode="😀 café"
escaped="a\"b\\c"
character='λ'
{- multiline
       unchanged interior
-}
final= { "do":true, where:false }
-- EOF comment
