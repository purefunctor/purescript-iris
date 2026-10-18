module Main where

import Control.Applicative (pure)
import Control.Bind (bind)
import Effect (Effect)

data Choice = First | Second

foreign import constructEffect :: forall value. String -> value -> Effect value

branchResult :: Boolean -> String -> Effect String
branchResult condition seed = do
  selected <-
    case condition of
      true -> constructEffect "branch-then" seed
      false -> constructEffect "branch-else" seed
  constructEffect "branch-after" selected

caseResult :: Choice -> String -> Effect String
caseResult choice seed = do
  selected <- case choice of
    First -> constructEffect "case-first" seed
    Second -> constructEffect "case-second" seed
  constructEffect "case-after" selected

guardResult :: Boolean -> String -> Effect String
guardResult condition seed = do
  selected <- case seed of
    value | condition -> constructEffect "guard-true" value
    value -> constructEffect "guard-false" value
  constructEffect "guard-after" selected
