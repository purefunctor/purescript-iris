module Main where

import Iris.Effect (getService)
import Services (Console(..), Http(..))

console :: Record (logger :: Console, network :: Http) -> Console
console = getService

http :: Record (logger :: Console, network :: Http) -> Http
http = getService

misleading :: Record (console :: Http, http :: Console) -> Console
misleading = getService

type ConsoleAlias = Console

aliased :: Record (renamed :: ConsoleAlias) -> Console
aliased = getService

consoleValue :: Console
consoleValue = console { logger: Console 19, network: Http 23 }

httpValue :: Http
httpValue = http { logger: Console 19, network: Http 23 }

misleadingValue :: Console
misleadingValue = misleading { console: Http 9, http: Console 42 }

aliasedValue :: Console
aliasedValue = aliased { renamed: Console 51 }
