module Main where

import Record as Record

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

withDefaults :: { y :: String } -> { x :: Int, y :: String }
withDefaults options = Record.merge options { x: 1, y: "default" }

override :: { x :: Int, y :: String } -> { x :: Int, y :: String }
override record = Record.merge { x: 0 } record

union :: { y :: String } -> { y :: String, x :: Int }
union record = Record.union record { x: 1 }

disjointUnion :: { y :: String } -> { x :: Int, y :: String }
disjointUnion record = Record.disjointUnion { x: 1 } record

literals :: { x :: Int, y :: String }
literals = Record.merge { x: 1 } { y: "right", x: 2 }

withDefaultsOrder :: { y :: String } -> { x :: Int, y :: String }
withDefaultsOrder options =
  Record.merge (observe "options" options) { x: observe "default" 1, y: "default" }

overrideOrder :: { x :: Int, y :: String } -> { x :: Int, y :: String }
overrideOrder record = Record.merge { x: observe "override" 0 } (observe "record" record)

literalsOrder :: Boolean -> { x :: Int, y :: String }
literalsOrder _ = Record.merge { x: observe "left" 1 } { y: "right", x: observe "right" 2 }

overrideBranchOrder :: { x :: Int, y :: String } -> { x :: Int, y :: String }
overrideBranchOrder record =
  Record.merge
    { y: observe "override" "y"
    , x: case record.y of
        "branch" -> observe "branch" 1
        _ -> observe "fallback" 2
    }
    record
