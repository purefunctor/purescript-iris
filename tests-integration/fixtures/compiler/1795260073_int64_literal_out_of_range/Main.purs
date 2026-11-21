module Main where

import Int64Literal (int64Digits)

aboveMaximum :: String
aboveMaximum = int64Digits @9223372036854775808

belowMinimum :: String
belowMinimum = int64Digits @(-9223372036854775809)
