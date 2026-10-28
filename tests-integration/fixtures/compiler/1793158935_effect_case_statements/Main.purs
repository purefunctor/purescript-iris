module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref
import Partial.Unsafe (unsafePartial)

data Choice = First | Second Int | Third

foreign import observe :: Choice -> Choice
foreign import record :: String -> Effect Unit
foreign import takeLog :: Effect (Array String)

statementCase :: Choice -> Effect Int
statementCase choice = do
  ref <- Ref.new 0
  case observe choice of
    First -> Ref.write 1 ref
    Second count -> Ref.write count ref
    Third -> pure unit
  Ref.modify_ (_ + 10) ref
  Ref.read ref

guardedCase :: Choice -> Effect Int
guardedCase choice = do
  record "before"
  value <- case choice of
    Second count
      | count > 0 -> pure count
      | otherwise -> do
          record "negative"
          pure 0
    _ -> pure (-1)
  record "after"
  pure value

partialCase :: Choice -> Effect Unit
partialCase = unsafePartial \choice -> do
  record "before"
  case choice of
    First -> record "first"
  record "after"
