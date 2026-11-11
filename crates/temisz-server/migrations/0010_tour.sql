-- The guided tour is shown at each user's first sign-in, on any device.
ALTER TABLE platform.accounts ADD COLUMN tour_seen_at TIMESTAMPTZ;
