import { useState, useEffect, useRef, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useI18n } from '../i18n';

interface CalibrationProps {
  onComplete: (yaw: number, pitch: number) => void;
  onFail: () => void;
  onCancel: () => void;
}

const DURATION_SECONDS = 5;
const TICK_MS = 100;

interface PoseUpdate {
  pose_state: string;
  yaw: number | null;
  pitch: number | null;
}

interface CalibrationCompletePayload {
  yaw: number;
  pitch: number;
  sample_count: number;
}

interface CalibrationFailedPayload {
  reason: string;
}

export function Calibration({ onComplete, onFail, onCancel }: CalibrationProps) {
  const { t } = useI18n();
  const [countdown, setCountdown] = useState(DURATION_SECONDS);
  const [samples, setSamples] = useState(0);
  const [lastYaw, setLastYaw] = useState<number | null>(null);
  const [lastPitch, setLastPitch] = useState<number | null>(null);

  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const unlistenPoseRef = useRef<(() => void) | null>(null);
  const unlistenCompleteRef = useRef<(() => void) | null>(null);
  const unlistenFailedRef = useRef<(() => void) | null>(null);
  const finishedRef = useRef(false);
  const shouldCancelOnUnmountRef = useRef(true);
  const handlersRef = useRef({ onComplete, onFail, onCancel });
  handlersRef.current = { onComplete, onFail, onCancel };

  const cleanup = useCallback(() => {
    finishedRef.current = true;
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    if (unlistenPoseRef.current) {
      unlistenPoseRef.current();
      unlistenPoseRef.current = null;
    }
    if (unlistenCompleteRef.current) {
      unlistenCompleteRef.current();
      unlistenCompleteRef.current = null;
    }
    if (unlistenFailedRef.current) {
      unlistenFailedRef.current();
      unlistenFailedRef.current = null;
    }
  }, []);

  useEffect(() => {
    finishedRef.current = false;
    shouldCancelOnUnmountRef.current = true;

    listen<PoseUpdate>('pose-updated', (event) => {
      if (finishedRef.current) return;
      const { yaw, pitch } = event.payload;
      if (yaw !== null && pitch !== null) {
        setLastYaw(yaw);
        setLastPitch(pitch);
      }
    }).then((unlisten) => {
      if (finishedRef.current) {
        unlisten();
      } else {
        unlistenPoseRef.current = unlisten;
      }
    });

    listen<CalibrationCompletePayload>('calibration-complete', (event) => {
      if (finishedRef.current) return;
      shouldCancelOnUnmountRef.current = false;
      cleanup();
      handlersRef.current.onComplete(event.payload.yaw, event.payload.pitch);
    }).then((unlisten) => {
      if (finishedRef.current) {
        unlisten();
      } else {
        unlistenCompleteRef.current = unlisten;
      }
    });

    listen<CalibrationFailedPayload>('calibration-failed', () => {
      if (finishedRef.current) return;
      shouldCancelOnUnmountRef.current = false;
      cleanup();
      handlersRef.current.onFail();
    }).then((unlisten) => {
      if (finishedRef.current) {
        unlisten();
      } else {
        unlistenFailedRef.current = unlisten;
      }
    });

    timerRef.current = setInterval(() => {
      setCountdown((prev) => {
        const next = prev - TICK_MS / 1000;
        return next <= 0 ? 0 : next;
      });
      setSamples((prev) => prev + 1);
    }, TICK_MS);

    return () => {
      cleanup();
      if (shouldCancelOnUnmountRef.current) {
        invoke('cancel_calibration').catch(() => {});
      }
    };
  }, [cleanup]);

  const handleCancel = useCallback(() => {
    shouldCancelOnUnmountRef.current = false;
    cleanup();
    invoke('cancel_calibration').catch(() => {});
    handlersRef.current.onCancel();
  }, [cleanup]);

  return (
    <div className="calibration">
      <p className="calibration-instruction">{t('calibration.starting')}</p>
      <div className="calibration-countdown">
        {t('calibration.countdown', { seconds: Math.ceil(countdown) })}
      </div>
      <div className="calibration-samples">
        {t('calibration.samples', { count: samples })}
      </div>
      {lastYaw !== null && (
        <div className="calibration-pose">
          yaw: {lastYaw.toFixed(1)}° / pitch: {lastPitch?.toFixed(1)}°
        </div>
      )}
      <button onClick={handleCancel} className="btn-secondary calibration-cancel">
        {t('calibration.cancel')}
      </button>
    </div>
  );
}
