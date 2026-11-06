module Main where

import Prelude

import Effect (Effect, forE, foreachE, untilE, whileE)
import Effect.Ref (Ref)
import Effect.Ref as Ref

foreign import mark :: forall value. String -> value -> value

fresh :: Effect (Ref Int)
fresh = Ref.new 0

forCaptured :: Ref Int -> Effect Unit
forCaptured counter =
  forE 0 (mark "high" 3) (mark "callback" \index -> Ref.modify_ (_ + index) counter)

forInlineBody :: Ref Int -> Effect Unit
forInlineBody counter = forE 0 3 \index -> Ref.write (mark "body" index) counter

untilCaptured :: Ref Int -> Effect Unit
untilCaptured counter = untilE (mark "action" (map (_ >= 2) (Ref.modify (_ + 1) counter)))

whileCaptured :: Ref Int -> Effect Unit
whileCaptured counter = whileE (mark "condition" (map (_ < 3) (Ref.read counter)))
  (mark "action" (Ref.modify_ (_ + 1) counter))

foreachCaptured :: Ref Int -> Array Int -> Effect Unit
foreachCaptured counter elements = foreachE (mark "elements" elements)
  (mark "callback" \element -> Ref.modify_ (_ + element) counter)

forPartial :: Int -> (Int -> Effect Unit) -> Effect Unit
forPartial count = forE 0 count
