-- @format width=80
module Main where

applied = consumeEveryArgument firstArgumentExpressionWithLongName secondArgumentExpressionWithLongName case someLongerInput of
  value -> value

appliedAlone = consumeEveryArgument firstArgumentExpressionWithLongName secondArgumentExpressionWithLongName case someEvenLongerInputExpressionValue of
  value -> value

chained = firstOperandExpression <> secondOperandExpression <> case someLongerInputExpression of
  value -> value

chainedAlone = firstOperandExpression <> secondOperandExpressionWithTheLongestName <> case someLongerInputExpression of
  value -> value
