module Main where

import Iris.Effect (class Has, getService, type (&))

data Logger

data Box :: Type -> Type
data Box value

type With service = service & Logger

invalid :: forall @service services. Has (service & Logger) services => Record services -> service
invalid = getService

valid :: forall value services. Has (Box value & Logger) services => Record services -> Box value
valid = getService
