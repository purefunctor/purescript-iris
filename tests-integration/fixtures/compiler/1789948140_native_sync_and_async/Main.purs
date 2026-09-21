module Main where

import Iris.Effect (Async, Fiber, Sync)
import Iris.Effect.Async as Async
import Iris.Effect.Sync as Sync
import Prim.Effect (Abort, class Subset)
import Prelude
import Wrapper as Wrapped

newtype DatabaseError = DatabaseError Int

newtype ValidationError = ValidationError Int

type FirstStructuralError = { a :: Int, b :: String }

type SecondStructuralError = { "a :: Prim.Int, b" :: String }

class Available :: Effects -> Constraint
class Available effects where
  available :: Int

instance availableInstance :: Subset [] effects => Available effects where
  available = 17

proofRetained :: forall @effects. Subset [] effects => Int
proofRetained = available @effects

proofRetention :: Int
proofRetention = proofRetained @[]

foreign import mark :: String -> Int -> Sync [] Int

foreign import delayed :: Int -> Async [] Int

syncProgram :: Sync [] Int
syncProgram = Sync.do
  first <- mark "first" 20
  second <- mark "second" 22
  Sync.pure (first + second)

wrappedProgram :: Sync [] Int
wrappedProgram = Wrapped.pure 21

asyncProgram :: Async [] Int
asyncProgram = Async.do
  value <- Async.lift syncProgram
  Async.yield
  delayed value

deep :: Int -> Int -> Async [] Int
deep remaining total =
  if remaining == 0 then Async.pure total
  else Async.defer \_ -> deep (remaining - 1) (total + 1)

main :: Sync [] (Fiber Int)
main = Async.run asyncProgram

runDeep :: Sync [] (Fiber Int)
runDeep = Async.run (deep 100000 0)

syncFailure :: Sync [Abort DatabaseError] Int
syncFailure = Sync.abort (DatabaseError 7)

syncRecovered :: Sync [] Int
syncRecovered =
  Sync.catchAbort syncFailure \(DatabaseError code) -> Sync.pure code

asyncFailures :: Async [Abort DatabaseError, Abort ValidationError] Int
asyncFailures = Async.abort (ValidationError 13)

asyncPartiallyHandled :: Async [Abort ValidationError] Int
asyncPartiallyHandled =
  Async.catchAbort asyncFailures \(DatabaseError _) -> Async.pure 0

asyncRecovered :: Async [] Int
asyncRecovered =
  Async.catchAbort asyncPartiallyHandled \(ValidationError code) -> Async.pure code

runAbort :: Sync [] (Fiber Int)
runAbort = Async.run asyncRecovered

structuralFailure :: Async [Abort FirstStructuralError] Int
structuralFailure = Async.abort { a: 1, b: "first" }

structuralRecovered :: Async [] Int
structuralRecovered =
  Async.catchAbort
    (Async.catchAbort structuralFailure \(_ :: SecondStructuralError) -> Async.pure 99)
    \(_ :: FirstStructuralError) -> Async.pure 42

runStructuralAbort :: Sync [] (Fiber Int)
runStructuralAbort = Async.run structuralRecovered
