import {
  createContext,
  useContext,
  useReducer,
  useEffect,
  type ReactNode,
  type Dispatch,
} from "react";
import type {
  DuoStatus,
  DuoSettings,
  Profile,
  HardwareEvent,
  VersionInfo,
} from "@/types/duo";
import { DEFAULT_DUO_SETTINGS, DEFAULT_DUO_STATUS } from "@/lib/defaults";
import { loadInitialStoreState, subscribeStoreEvents } from "@/lib/store-effects";
export {
  clearLogs,
  refreshEvents,
  refreshLogs,
  refreshProfiles,
  refreshSettings,
  refreshStatus,
  refreshVersionInfo,
} from "@/lib/store-effects";
import { APP_VERSION } from "@/lib/version";

export interface AppState {
  status: DuoStatus;
  settings: DuoSettings;
  profiles: Profile[];
  events: HardwareEvent[];
  logs: string[];
  versionInfo: VersionInfo;
  loading: boolean;
}

export type StoreAction =
  | { type: "SET_STATUS"; payload: DuoStatus }
  | { type: "SET_SETTINGS"; payload: DuoSettings }
  | { type: "SET_PROFILES"; payload: Profile[] }
  | { type: "SET_EVENTS"; payload: HardwareEvent[] }
  | { type: "SET_LOGS"; payload: string[] }
  | { type: "SET_VERSION_INFO"; payload: VersionInfo }
  | { type: "SET_LOADING"; payload: boolean };

const initialState: AppState = {
  status: DEFAULT_DUO_STATUS,
  settings: DEFAULT_DUO_SETTINGS,
  profiles: [],
  events: [],
  logs: [],
  versionInfo: {
    appVersion: APP_VERSION,
    appProtocolVersion: 0,
    daemonVersion: null,
    daemonProtocolVersion: null,
    serviceAvailable: false,
  },
  loading: true,
};

function reducer(state: AppState, action: StoreAction): AppState {
  switch (action.type) {
    case "SET_STATUS":
      return { ...state, status: action.payload };
    case "SET_SETTINGS":
      return { ...state, settings: action.payload };
    case "SET_PROFILES":
      return { ...state, profiles: action.payload };
    case "SET_EVENTS":
      return { ...state, events: action.payload };
    case "SET_LOGS":
      return { ...state, logs: action.payload };
    case "SET_VERSION_INFO":
      return { ...state, versionInfo: action.payload };
    case "SET_LOADING":
      return { ...state, loading: action.payload };
  }
}

const StoreContext = createContext<AppState>(initialState);
const DispatchContext = createContext<Dispatch<StoreAction>>(() => {});

export function StoreProvider({ children }: { children: ReactNode }) {
  const [state, dispatch] = useReducer(reducer, initialState);

  return (
    <StoreContext.Provider value={state}>
      <DispatchContext.Provider value={dispatch}>
        {children}
      </DispatchContext.Provider>
    </StoreContext.Provider>
  );
}

export function useStore() {
  return useContext(StoreContext);
}

export function useDispatch() {
  return useContext(DispatchContext);
}

export function useStoreInit() {
  const dispatch = useDispatch();

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    async function init() {
      dispatch({ type: "SET_LOADING", payload: true });

      await loadInitialStoreState(dispatch);

      dispatch({ type: "SET_LOADING", payload: false });

      unlisteners.push(...subscribeStoreEvents(dispatch));
    }

    init();

    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [dispatch]);
}
