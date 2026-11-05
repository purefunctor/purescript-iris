module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

foreign import mark :: forall value. String -> value -> value

foreign import peek :: Ref Int -> Int

fresh :: Effect (Ref Int)
fresh = Ref.new 0

writeArgument :: Ref Int -> Effect Int
writeArgument reference = do
  Ref.write (mark "write argument" 5) reference
  _ <- Ref.modify (mark "modify function" (_ + 1)) reference
  Ref.read reference

readLater :: Ref Int -> Effect Int
readLater reference = Ref.read reference

reusedRead :: Ref Int -> Effect { before :: Int, after :: Int }
reusedRead reference = do
  let
    read = Ref.read reference
  before <- read
  Ref.write 7 reference
  after <- read
  pure { before, after }

modifyOrder :: Ref Int -> Effect { returned :: Int, seen :: Int, state :: Int }
modifyOrder reference = do
  returned <-
    Ref.modify' (\old -> { state: old * 10, value: { returned: old + 1, seen: peek reference } })
      reference
  state <- Ref.read reference
  pure { returned: returned.returned, seen: returned.seen, state }

modifyResults :: Ref Int -> Effect { modified :: Int, discarded :: Unit, final :: Int }
modifyResults reference = do
  modified <- Ref.modify (_ + 1) reference
  discarded <- Ref.modify_ (_ * 2) reference
  final <- Ref.read reference
  pure { modified, discarded, final }

newPerExecution :: Effect Int
newPerExecution = do
  reference <- Ref.new 0
  Ref.modify (_ + 1) reference
