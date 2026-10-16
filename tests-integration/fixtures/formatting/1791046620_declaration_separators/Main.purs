-- @format width=80
-- @format width=100 indent=4 unicode=true
-- @format width=120
module Main where

data Command = Initialize Configuration | Execute Request Environment | Shutdown Reason | Restart

guarded value
  | Just first <- lookupFirst value, Just second <- lookupSecond first, second > threshold = second

class Convert inputType outputType intermediateType | inputType -> outputType, outputType -> intermediateType, intermediateType -> inputType

class Correlate firstParameter secondParameter finalParameter | firstParameter secondParameter -> finalParameter

class Multiple firstLongVariable secondLongVariable thirdLongVariable fourthLongVariable fifthLongVariable | firstLongVariable secondLongVariable -> thirdLongVariable fourthLongVariable fifthLongVariable

instance command :: (Show configuration, Eq environment, Ord configuration, Monoid environment) => Show (Command configuration environment) where
  show value = "command"
