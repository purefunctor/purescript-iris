module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

data Expression
  = Literal Int
  | Add Expression Expression
  | Parenthesised Expression
  | Sum Terms

data Terms = Nil | Cons Expression Terms

evaluate :: Expression -> Int
evaluate (Literal value) = value
evaluate (Add left right) = evaluate left + evaluate right
evaluate (Parenthesised expression) = evaluate expression
evaluate (Sum terms) = sumTerms 0 terms

sumTerms :: Int -> Terms -> Int
sumTerms total Nil = total
sumTerms total (Cons expression rest) = sumTerms (total + evaluate expression) rest

countToZero :: Ref Int -> Effect Int
countToZero counter = advance 1
  where
  advance step = do
    Ref.modify_ (_ + step) counter
    settle step

  settle step = do
    count <- Ref.read counter
    if count < 0 then do
      _ <- advance step
      pure count
    else pure count

skipToZero :: Ref Int -> Int -> Effect Int
skipToZero counter = skip
  where
  skip depth
    | depth > 0 = skip (depth - 1)
    | otherwise = do
        Ref.modify_ (_ + 1) counter
        settle depth

  settle depth = do
    count <- Ref.read counter
    if count < 0 then do
      _ <- skip depth
      pure count
    else pure count
