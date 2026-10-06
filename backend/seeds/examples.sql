-- Example artifacts for onboarding authors.
--
-- Two real campus buildings, each with child artifacts, so new authors can
-- see what a well-described artifact looks like and how the building ->
-- feature hierarchy works (children inherit their parent's location).
--
-- Every name starts with "Example:" so they are easy to spot and to remove.
-- Facts come from the UMass pages cited at the end of each description.
--
-- Safe to run more than once: fixed ids and ON CONFLICT DO NOTHING.
-- Remove with seeds/remove-examples.sql.
--
-- Run on the server from ~/pintrail/backend:
--   docker compose exec -T postgres psql -U pintrail -d pintrail -v ON_ERROR_STOP=1 < seeds/examples.sql

BEGIN;

INSERT INTO artifacts (id, kind, name, description, lat, lng, parent_id) VALUES

-- Integrative Learning Center ---------------------------------------------
('5eed0000-0000-4000-8000-000000000001', 'building',
 'Example: Integrative Learning Center',
 'A 173,000-square-foot classroom and department building at 650 North Pleasant Street, completed in summer 2014 and certified LEED Gold (LEED NC v2009).

Why it matters: the building was designed to use about a third less energy than a standard design (a 34% reduction against the baseline), to cut drinking-water use by a projected 39%, and to manage 90% of its stormwater on site. During construction, 80% of the waste was kept out of landfill, and 21% of the materials were sourced regionally.

Look for the features listed under this building: they are separate artifacts, each with its own story.

Source: UMass Planning, Design & Construction, "Integrative Learning Center (2014)", https://www.umass.edu/planning-design-construction/book/integrative-learning-center-2014',
 42.3910382, -72.5259025, NULL),

('5eed0000-0000-4000-8000-000000000002', 'rooftop',
 'Example: Green roof',
 'The roof of the Integrative Learning Center is planted with native, hardy plant species instead of bare roofing.

How it works: the plants and growing medium absorb carbon dioxide, reduce glare, and hold rainwater so less of it rushes off the building in a storm. It is one of the reasons the building can manage most of its stormwater on site.

Location: no coordinates of its own; it inherits them from the building.

Source: UMass Planning, Design & Construction, "Integrative Learning Center (2014)", https://www.umass.edu/planning-design-construction/book/integrative-learning-center-2014',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000001'),

('5eed0000-0000-4000-8000-000000000003', 'installation',
 'Example: Stormwater reuse for irrigation',
 'Rain that falls on the Integrative Learning Center site is collected and reused to water the landscaping, rather than using drinking water.

Why it matters: stormwater reuse accounts for a 64% reduction in water used for irrigation. Together with the green roof, the site manages 90% of its stormwater on site instead of sending it into storm drains.

Location: no coordinates of its own; it inherits them from the building.

Source: UMass Planning, Design & Construction, "Integrative Learning Center (2014)", https://www.umass.edu/planning-design-construction/book/integrative-learning-center-2014',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000001'),

-- Central Heating Plant ---------------------------------------------------
('5eed0000-0000-4000-8000-000000000010', 'building',
 'Example: Central Heating Plant',
 'The campus power and steam plant at 200 Mullins Way, completed in 2009.

Why it matters: it generates electricity for about 70% of the campus and supplies all of the steam used to heat and cool campus buildings. Making electricity and heat in the same place (combined heat and power) uses fuel far more efficiently than buying electricity and running separate boilers.

Look for the systems listed under this building.

Source: UMass Facilities & Campus Services, "Utilities", https://www.umass.edu/facilities/utilities',
 42.3897530, -72.5370083, NULL),

('5eed0000-0000-4000-8000-000000000011', 'installation',
 'Example: Combined heat and power system',
 'The heart of the Central Heating Plant: a 10-megawatt combustion turbine, a heat recovery steam generator, and a 4-megawatt steam turbine, backed up by three natural gas boilers.

How it works: the combustion turbine burns natural gas to make electricity. Instead of wasting its hot exhaust, the heat recovery steam generator turns that heat into steam. Some steam drives the steam turbine to make more electricity; the rest heats and cools campus buildings.

Location: no coordinates of its own; it inherits them from the building.

Source: UMass Facilities & Campus Services, "Utilities", https://www.umass.edu/facilities/utilities',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000010'),

('5eed0000-0000-4000-8000-000000000012', 'installation',
 'Example: Battery energy storage system',
 'A 1-megawatt / 4-megawatt-hour lithium-ion battery at the Central Heating Plant, funded by a $1.1 million state grant from the Advancing Commonwealth Energy Storage (ACES) program in December 2017.

Why it matters: the battery stores electricity when demand is low and releases it at peak times, which lowers the campus''s peak demand, helps integrate renewable sources such as solar, and supports grid stability and resilience.

Location: no coordinates of its own; it inherits them from the building.

Source: UMass Clean Energy Extension, "UMass Amherst Energy Storage Project", https://www.umass.edu/agriculture-food-environment/clean-energy/current-initiatives/energy-storage/umass-amherst-energy-storage-project',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000010')

ON CONFLICT (id) DO NOTHING;

COMMIT;
