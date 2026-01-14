-- Add fallback password field for when keychain fails
-- This stores base64-encoded password as a fallback

ALTER TABLE endpoints ADD COLUMN password_fallback TEXT;
