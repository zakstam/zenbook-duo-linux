import {
  createContext,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
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
import { shallowEqual } from "@/lib/utils";

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
      // The backend sends a fresh object on every refresh; keep the old one
      // when nothing changed so readers of status do not re-render.
      if (shallowEqual(state.status, action.payload)) return state;
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
      if (shallowEqual(state.versionInfo, action.payload)) return state;
      return { ...state, versionInfo: action.payload };
    case "SET_LOADING":
      return { ...state, loading: action.payload };
  }
}

interface StoreApi {
  getState: () => AppState;
  subscribe: (listener: () => void) => () => void;
  dispatch: Dispatch<StoreAction>;
}

function createStoreApi(): StoreApi {
  let state = initialState;
  const listeners = new Set<() => void>();
  return {
    getState: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    dispatch(action) {
      const next = reducer(state, action);
      if (next === state) return;
      state = next;
      listeners.forEach((listener) => listener());
    },
  };
}

const StoreApiContext = createContext<StoreApi>(createStoreApi());

export function StoreProvider({ children }: { children: ReactNode }) {
  const [api] = useState(createStoreApi);

  return <StoreApiContext.Provider value={api}>{children}</StoreApiContext.Provider>;
}

/**
 * Read one piece of the store. The component re-renders only when the
 * selected value changes (by identity), so select the slice or field you
 * show, never a freshly built object.
 */
export function useStoreSelector<T>(selector: (state: AppState) => T): T {
  const api = useContext(StoreApiContext);
  return useSyncExternalStore(api.subscribe, () => selector(api.getState()));
}

export function useDispatch() {
  return useContext(StoreApiContext).dispatch;
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
