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
} from "@/types/duo";
import * as api from "@/lib/tauri";

export interface AppState {
  status: DuoStatus;
  settings: DuoSettings;
  profiles: Profile[];
  events: HardwareEvent[];
  logs: string[];
  loading: boolean;
}

type Action =
  | { type: "SET_STATUS"; payload: DuoStatus }
  | { type: "SET_SETTINGS"; payload: DuoSettings }
  | { type: "SET_PROFILES"; payload: Profile[] }
  | { type: "SET_EVENTS"; payload: HardwareEvent[] }
  | { type: "SET_LOGS"; payload: string[] }
  | { type: "SET_LOADING"; payload: boolean };

const defaultStatus: DuoStatus = {
  keyboardAttached: false,
  connectionType: "none",
  monitorCount: 0,
  wifiEnabled: false,
  bluetoothEnabled: false,
  backlightLevel: 0,
  displayBrightness: 0,
  maxBrightness: 1,
  serviceActive: false,
  orientation: "normal",
};

const defaultSettings: DuoSettings = {
  defaultBacklight: 3,
  defaultScale: 1.66,
  autoDualScreen: true,
  syncBrightness: true,
  theme: "system",
};

const initialState: AppState = {
  status: defaultStatus,
  settings: defaultSettings,
  profiles: [],
  events: [],
  logs: [],
  loading: true,
};

function reducer(state: AppState, action: Action): AppState {
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
    case "SET_LOADING":
      return { ...state, loading: action.payload };
  }
}

const StoreContext = createContext<AppState>(initialState);
const DispatchContext = createContext<Dispatch<Action>>(() => {});

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

export async function refreshStatus(dispatch: Dispatch<Action>) {
  try {
    const status = await api.getStatus();
    dispatch({ type: "SET_STATUS", payload: status });
  } catch (e) {
    console.error("Failed to fetch status:", e);
  }
}

export async function refreshSettings(dispatch: Dispatch<Action>) {
  try {
    const settings = await api.loadSettings();
    dispatch({ type: "SET_SETTINGS", payload: settings });
  } catch (e) {
    console.error("Failed to load settings:", e);
  }
}

export async function refreshProfiles(dispatch: Dispatch<Action>) {
  try {
    const profiles = await api.listProfiles();
    dispatch({ type: "SET_PROFILES", payload: profiles });
  } catch (e) {
    console.error("Failed to load profiles:", e);
  }
}

export async function refreshLogs(dispatch: Dispatch<Action>) {
  try {
    const logs = await api.readLog(500);
    dispatch({ type: "SET_LOGS", payload: logs });
  } catch (e) {
    console.error("Failed to read logs:", e);
  }
}

export async function clearLogs(dispatch: Dispatch<Action>) {
  try {
    await api.clearLog();
    dispatch({ type: "SET_LOGS", payload: [] });
  } catch (e) {
    console.error("Failed to clear logs:", e);
  }
}

export async function refreshEvents(dispatch: Dispatch<Action>) {
  try {
    const events = await api.getRecentEvents(100);
    dispatch({ type: "SET_EVENTS", payload: events });
  } catch (e) {
    console.error("Failed to get events:", e);
  }
}

export function useStoreInit() {
  const dispatch = useDispatch();

  useEffect(() => {
    const unlisteners: Promise<() => void>[] = [];

    async function init() {
      dispatch({ type: "SET_LOADING", payload: true });

      await Promise.all([
        refreshStatus(dispatch),
        refreshSettings(dispatch),
        refreshProfiles(dispatch),
        refreshLogs(dispatch),
      ]);

      dispatch({ type: "SET_LOADING", payload: false });

      unlisteners.push(api.onStatusChanged(() => refreshStatus(dispatch)));
      unlisteners.push(api.onLogUpdated(() => refreshLogs(dispatch)));
      unlisteners.push(api.onHardwareEvent(() => refreshEvents(dispatch)));
    }

    init();

    return () => {
      unlisteners.forEach((p) => p.then((unlisten) => unlisten()));
    };
  }, [dispatch]);
}
