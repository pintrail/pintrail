-- "Getting here": the trail author's own directions to each stop from the
-- previous one (for the first stop, how to reach the start). Automatic
-- turn-by-turn is weak on campus, where many paths have no name, so a short
-- human note is what makes a trail walkable. Optional; same limit as note.
ALTER TABLE trail_stops
    ADD COLUMN directions TEXT,
    ADD CONSTRAINT trail_stops_directions_length CHECK (char_length(directions) <= 1000);
