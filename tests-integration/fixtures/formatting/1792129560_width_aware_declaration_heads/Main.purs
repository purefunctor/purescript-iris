-- @format width=80 indent=2
-- @format width=100 indent=2
-- @format width=120 indent=2
-- @format width=80 indent=4
-- @format width=80 indent=2 unicode=true
module Main where

data Box value = Empty | Box value | Pair value value | Many value value value | Nested (Box value)

longApplication :: forall value. Collection value -> Collection value -> Collection value -> Collection value -> Collection value
longApplication firstCollection secondCollection thirdCollection fourthCollection = combineCollections firstCollection secondCollection thirdCollection fourthCollection

rows :: { firstField :: Collection Int, secondField :: Collection String, thirdField :: Collection Boolean | tail }
rows = value :: Collection String

constrained :: forall first second. LongConstraint first => OtherConstraint second => first -> second -> Result first second
