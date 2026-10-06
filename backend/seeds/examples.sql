-- Example artifacts for onboarding authors.
--
-- Two real campus buildings, each with child artifacts, so new authors can
-- see what a well-described artifact looks like and how the building ->
-- feature hierarchy works (children inherit their parent's location).
--
-- Every name starts with "Example:" so they are easy to spot and to remove.
-- Facts come from the UMass pages attached to each artifact as links, and
-- each artifact carries a few tags, so the examples show every part of the
-- form. The link previews are filled in here from each page's own metadata
-- (as of October 2026), the same fields the Studio reads when an author adds
-- a link.
--
-- Safe to run more than once: fixed ids and ON CONFLICT DO NOTHING. That also
-- means re-running it does not update examples already loaded; to pick up a
-- change to this file, run seeds/remove-examples.sql first.
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

Look for the features listed under this building: they are separate artifacts, each with its own story.',
 42.3910382, -72.5259025, NULL),

('5eed0000-0000-4000-8000-000000000002', 'rooftop',
 'Example: Green roof',
 'The roof of the Integrative Learning Center is planted with native, hardy plant species instead of bare roofing.

How it works: the plants and growing medium absorb carbon dioxide, reduce glare, and hold rainwater so less of it rushes off the building in a storm. It is one of the reasons the building can manage most of its stormwater on site.

Location: no coordinates of its own; it inherits them from the building.',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000001'),

('5eed0000-0000-4000-8000-000000000003', 'installation',
 'Example: Stormwater reuse for irrigation',
 'Rain that falls on the Integrative Learning Center site is collected and reused to water the landscaping, rather than using drinking water.

Why it matters: stormwater reuse accounts for a 64% reduction in water used for irrigation. Together with the green roof, the site manages 90% of its stormwater on site instead of sending it into storm drains.

Location: no coordinates of its own; it inherits them from the building.',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000001'),

-- Central Heating Plant ---------------------------------------------------
('5eed0000-0000-4000-8000-000000000010', 'building',
 'Example: Central Heating Plant',
 'The campus power and steam plant at 200 Mullins Way, completed in 2009.

Why it matters: it generates electricity for about 70% of the campus and supplies all of the steam used to heat and cool campus buildings. Making electricity and heat in the same place (combined heat and power) uses fuel far more efficiently than buying electricity and running separate boilers.

Look for the systems listed under this building.',
 42.3897530, -72.5370083, NULL),

('5eed0000-0000-4000-8000-000000000011', 'installation',
 'Example: Combined heat and power system',
 'The heart of the Central Heating Plant: a 10-megawatt combustion turbine, a heat recovery steam generator, and a 4-megawatt steam turbine, backed up by three natural gas boilers.

How it works: the combustion turbine burns natural gas to make electricity. Instead of wasting its hot exhaust, the heat recovery steam generator turns that heat into steam. Some steam drives the steam turbine to make more electricity; the rest heats and cools campus buildings.

Location: no coordinates of its own; it inherits them from the building.',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000010'),

('5eed0000-0000-4000-8000-000000000012', 'installation',
 'Example: Battery energy storage system',
 'A 1-megawatt / 4-megawatt-hour lithium-ion battery at the Central Heating Plant, funded by a $1.1 million state grant from the Advancing Commonwealth Energy Storage (ACES) program in December 2017.

Why it matters: the battery stores electricity when demand is low and releases it at peak times, which lowers the campus''s peak demand, helps integrate renewable sources such as solar, and supports grid stability and resilience.

Location: no coordinates of its own; it inherits them from the building.',
 NULL, NULL, '5eed0000-0000-4000-8000-000000000010')

ON CONFLICT (id) DO NOTHING;

-- Tags --------------------------------------------------------------------
INSERT INTO artifact_tags (artifact_id, tag) VALUES
('5eed0000-0000-4000-8000-000000000001', 'green building'),
('5eed0000-0000-4000-8000-000000000001', 'LEED'),
('5eed0000-0000-4000-8000-000000000001', 'water'),
('5eed0000-0000-4000-8000-000000000002', 'green roof'),
('5eed0000-0000-4000-8000-000000000002', 'stormwater'),
('5eed0000-0000-4000-8000-000000000003', 'stormwater'),
('5eed0000-0000-4000-8000-000000000003', 'water'),
('5eed0000-0000-4000-8000-000000000010', 'energy'),
('5eed0000-0000-4000-8000-000000000010', 'heating and cooling'),
('5eed0000-0000-4000-8000-000000000011', 'energy'),
('5eed0000-0000-4000-8000-000000000011', 'combined heat and power'),
('5eed0000-0000-4000-8000-000000000012', 'energy'),
('5eed0000-0000-4000-8000-000000000012', 'energy storage')
ON CONFLICT DO NOTHING;

-- Links (sources) ----------------------------------------------------------
INSERT INTO artifact_links (id, artifact_id, url, note, position,
    preview_title, preview_description, preview_image_url, preview_site_name, preview_fetched_at) VALUES
('5eed0000-0000-4000-8000-000000000101', '5eed0000-0000-4000-8000-000000000001',
 'https://www.umass.edu/planning-design-construction/book/integrative-learning-center-2014',
 'Source for the energy, water, and LEED figures', 0,
 'Integrative Learning Center (2014) : Planning, Design & Construction : UMass Amherst',
 NULL, 'https://www.umass.edu/static/branding/images/og_default_image.png', 'Planning, Design & Construction', now()),
('5eed0000-0000-4000-8000-000000000102', '5eed0000-0000-4000-8000-000000000002',
 'https://www.umass.edu/planning-design-construction/book/integrative-learning-center-2014',
 'Source: describes the green roof', 0,
 'Integrative Learning Center (2014) : Planning, Design & Construction : UMass Amherst',
 NULL, 'https://www.umass.edu/static/branding/images/og_default_image.png', 'Planning, Design & Construction', now()),
('5eed0000-0000-4000-8000-000000000103', '5eed0000-0000-4000-8000-000000000003',
 'https://www.umass.edu/planning-design-construction/book/integrative-learning-center-2014',
 'Source for the stormwater figures', 0,
 'Integrative Learning Center (2014) : Planning, Design & Construction : UMass Amherst',
 NULL, 'https://www.umass.edu/static/branding/images/og_default_image.png', 'Planning, Design & Construction', now()),
('5eed0000-0000-4000-8000-000000000110', '5eed0000-0000-4000-8000-000000000010',
 'https://www.umass.edu/facilities/utilities',
 'Source: what the plant supplies to campus', 0,
 'Utilities : Facilities & Campus Services : UMass Amherst',
 'utilities facilities management', 'https://www.umass.edu/static/branding/images/og_default_image.png', 'Facilities & Campus Services', now()),
('5eed0000-0000-4000-8000-000000000111', '5eed0000-0000-4000-8000-000000000011',
 'https://www.umass.edu/facilities/utilities',
 'Source for the turbine and boiler details', 0,
 'Utilities : Facilities & Campus Services : UMass Amherst',
 'utilities facilities management', 'https://www.umass.edu/static/branding/images/og_default_image.png', 'Facilities & Campus Services', now()),
('5eed0000-0000-4000-8000-000000000112', '5eed0000-0000-4000-8000-000000000012',
 'https://www.umass.edu/agriculture-food-environment/clean-energy/current-initiatives/energy-storage/umass-amherst-energy-storage-project',
 'Source: the ACES grant and battery specifications', 0,
 'UMass Amherst Energy Storage Project : Clean Energy : Center for Agriculture, Food, and the Environment (CAFE) at UMass Amherst',
 'Massachusetts ACES Demonstration Project In December 2017, UMass Amherst was awarded a $1.1 million state grant from the Advancing Commonwealth Energy Storage (ACES) program to work with an energy storage company to construct a large battery at the Central Heating Plant on campus. UMass Amherst wil…', 'https://www.umass.edu/static/branding/images/og_default_image.png', 'Center for Agriculture, Food, and the Environment at UMass Amherst', now())
ON CONFLICT (id) DO NOTHING;

COMMIT;
