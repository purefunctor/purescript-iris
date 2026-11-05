import * as Iris_Effect_Http from "../Iris.Effect.Http/index.js";
import * as $foreign from "./foreign.js";
const keepAliveClient = $foreign["keepAliveClient"];
export const client = Iris_Effect_Http.fromForeignClient(keepAliveClient);
