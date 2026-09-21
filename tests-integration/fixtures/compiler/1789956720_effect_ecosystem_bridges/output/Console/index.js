import * as $foreign from "./foreign.js";
import * as $effect from "../effect.js";
export function log(message) {
  return $effect.syncLiftEffect(logEffect(message));
}
const logEffect = $foreign["logEffect"];
