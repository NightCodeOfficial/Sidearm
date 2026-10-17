import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppConfig, Profile } from "../lib/config";
import { upsertProfile } from "../lib/config-editing";
import {
  getMouseSettings,
  listenMouseBattery,
  normalizeCommandError,
  setMouseDpi,
  setMousePollRate,
} from "../lib/backend";
import {
  MOUSE_DPI_MAX,
  MOUSE_DPI_MIN,
  MOUSE_POLL_RATES,
  type MouseSettings as MouseSettingsState,
} from "../lib/onboard-mapping";
import { PillTrack } from "./PillTrack";
import { Notice, Toggle } from "./shared";

export interface MouseWorkspaceProps {
  activeConfig: AppConfig;
  /** Profiles in the sidebar's order. */
  profiles: Profile[];
  /** The profile being edited — the same selection as the sidebar. */
  activeProfile: Profile | null;
  /** The profile the runtime applies now: its edits reach the mouse at once. */
  runtimeProfileId: string | null;
  setSelectedProfileId: (id: string | null) => void;
  updateDraft: (updater: (config: AppConfig) => AppConfig) => void;
}

/** The "Mouse" workspace: a profile switcher on top, the selected profile's own
 *  DPI / poll rate (written into the mouse on every switch to that profile), and
 *  the battery with the tray-badge switch. */
export function MouseWorkspace({
  activeConfig,
  profiles,
  activeProfile,
  runtimeProfileId,
  setSelectedProfileId,
  updateDraft,
}: MouseWorkspaceProps) {
  const { t } = useTranslation();
  const [device, setDevice] = useState<MouseSettingsState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dpiDraft, setDpiDraft] = useState("");

  async function run(request: () => Promise<MouseSettingsState>) {
    setBusy(true);
    try {
      setDevice(await request());
      setError(null);
    } catch (unknownError) {
      setError(normalizeCommandError(unknownError).message);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void run(getMouseSettings);
    let unlisten: (() => void) | undefined;
    void listenMouseBattery(({ percent }) => {
      setDevice((current) => (current ? { ...current, batteryPercent: percent } : current));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const profileDpi = activeProfile?.mouseDpi;
  const profilePoll = activeProfile?.mousePollRateHz;
  const overrides = profileDpi !== undefined || profilePoll !== undefined;
  const appliesNow = activeProfile !== null && activeProfile.id === runtimeProfileId;

  useEffect(() => {
    setDpiDraft(profileDpi !== undefined ? String(profileDpi) : "");
  }, [profileDpi, activeProfile?.id]);

  function saveProfile(patch: Pick<Profile, "mouseDpi" | "mousePollRateHz">) {
    if (!activeProfile) return;
    updateDraft((config) => upsertProfile(config, { ...activeProfile, ...patch }));
  }

  /** Writes a value to the mouse when the edited profile is the one in effect. */
  function applyNow(request: () => Promise<MouseSettingsState>) {
    if (appliesNow) void run(request);
  }

  const dpiValue = Number(dpiDraft);
  const dpiValid =
    Number.isInteger(dpiValue) && dpiValue >= MOUSE_DPI_MIN && dpiValue <= MOUSE_DPI_MAX;

  return (
    <div className="settings-shell__content">
      <section className="settings-section">
        <div className="settings-section__header">
          <span className="settings-section__title">{t("mouse.profilePicker")}</span>
        </div>
        <PillTrack
          items={profiles.map((profile) => ({
            key: profile.id,
            label:
              profile.id === runtimeProfileId
                ? t("mouse.profileActiveNow", { name: profile.name })
                : profile.name,
          }))}
          active={activeProfile?.id ?? ""}
          onSelect={(id) => setSelectedProfileId(id)}
        />
        <p className="panel__muted help-caption mt-8">{t("mouse.profilePickerHelp")}</p>
      </section>

      {activeProfile ? (
        <section className="settings-section">
          <div className="settings-section__header">
            <span className="settings-section__title">
              {t("mouse.profileHeader", { name: activeProfile.name })}
            </span>
            <div className="settings-section__master">
              <span className="settings-section__master-label">{t("mouse.profileOverride")}</span>
              <Toggle
                checked={overrides}
                onChange={(checked) =>
                  saveProfile(
                    checked
                      ? { mouseDpi: device?.dpi ?? 1600, mousePollRateHz: device?.pollRateHz ?? 1000 }
                      : { mouseDpi: undefined, mousePollRateHz: undefined },
                  )
                }
                ariaLabel={t("mouse.profileOverride")}
              />
            </div>
          </div>
          <p className="panel__muted help-sm">
            {overrides ? t("mouse.profileHelpOn") : t("mouse.profileHelpOff")}
          </p>

          {overrides ? (
            <>
              <div className="osd-settings-row mt-12">
                <span className="osd-settings-row__label">{t("mouse.pollRate")}</span>
                <p className="panel__muted help-caption">{t("mouse.pollRateCaption")}</p>
                <PillTrack
                  items={MOUSE_POLL_RATES.map((hz) => ({
                    key: String(hz),
                    label: t("mouse.pollRateOption", { hz }),
                  }))}
                  active={String(profilePoll ?? "")}
                  onSelect={(key) => {
                    const hz = Number(key);
                    saveProfile({ mouseDpi: profileDpi, mousePollRateHz: hz });
                    applyNow(() => setMousePollRate(hz));
                  }}
                />
              </div>

              <div className="osd-settings-row mt-12">
                <label className="field">
                  <span className="field__label">{t("mouse.dpi")}</span>
                  <div className="input-suffix">
                    <input
                      type="number"
                      min={MOUSE_DPI_MIN}
                      max={MOUSE_DPI_MAX}
                      step={50}
                      value={dpiDraft}
                      onChange={(e) => setDpiDraft(e.target.value)}
                    />
                    <span className="input-suffix__unit">{t("mouse.dpiUnit")}</span>
                  </div>
                </label>
                <p className="panel__muted help-caption">
                  {t("mouse.dpiCaption", { min: MOUSE_DPI_MIN, max: MOUSE_DPI_MAX })}
                </p>
                <button
                  type="button"
                  className="action-button action-button--secondary action-button--small mt-8"
                  onClick={() => {
                    saveProfile({ mouseDpi: dpiValue, mousePollRateHz: profilePoll });
                    applyNow(() => setMouseDpi(dpiValue));
                  }}
                  disabled={busy || !dpiValid || dpiValue === profileDpi}
                >
                  {t("mouse.apply")}
                </button>
              </div>
            </>
          ) : null}
        </section>
      ) : null}

      <section className="settings-section">
        <div className="settings-section__header">
          <span className="settings-section__title">{t("mouse.batteryHeader")}</span>
          <div className="settings-section__master">
            <span className="settings-section__master-label">{t("mouse.batteryInTray")}</span>
            <Toggle
              checked={activeConfig.settings.showBatteryInTray ?? false}
              onChange={(checked) =>
                updateDraft((config) => ({
                  ...config,
                  settings: { ...config.settings, showBatteryInTray: checked || undefined },
                }))
              }
              ariaLabel={t("mouse.batteryInTray")}
            />
          </div>
        </div>
        <p className="panel__muted help-sm">{t("mouse.batteryHelp")}</p>
        {device ? (
          <p className="mt-8">
            <strong>{t("mouse.batteryValue", { percent: device.batteryPercent })}</strong>
          </p>
        ) : null}
        {error ? (
          <Notice variant="warning" className="mt-12">
            <p>{t("mouse.unavailable")}</p>
            <p className="panel__muted help-caption">{error}</p>
            <button
              type="button"
              className="action-button action-button--secondary action-button--small mt-8"
              onClick={() => void run(getMouseSettings)}
              disabled={busy}
            >
              {t("mouse.retry")}
            </button>
          </Notice>
        ) : null}
      </section>
    </div>
  );
}
