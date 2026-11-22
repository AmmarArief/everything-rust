<?php

namespace XmlUtil;

require_once __DIR__ . '/XML/Util.php';

/**
 * @param string $string
 * @param int $replaceEntities
 * @return string
 */
function replace_entities(string $string, int $replaceEntities = 1): string
{
    return \XML_Util::replaceEntities($string, $replaceEntities);
}

/**
 * @param string $string
 * @param int $replaceEntities
 * @return string
 */
function reverse_entities(string $string, int $replaceEntities = 1): string
{
    return \XML_Util::reverseEntities($string, $replaceEntities);
}

/**
 * @param array<string, string> $attributes
 * @return string
 */
function attributes_to_string(
    array $attributes,
    bool $sort = true,
    bool $multiline = false,
    string $indent = '    ',
    string $linebreak = "\n",
    int $entities = 1
): string {
    return \XML_Util::attributesToString($attributes, $sort, $multiline, $indent, $linebreak, $entities);
}

/**
 * @return array<string, string|null>
 */
function split_qualified_name(string $qname, ?string $defaultNs = null): array
{
    return \XML_Util::splitQualifiedName($qname, $defaultNs);
}

function create_comment(string $content): string
{
    return \XML_Util::createComment($content);
}

function create_cdata_section(string $data): string
{
    return \XML_Util::createCDataSection($data);
}

function create_end_element(string $qname): string
{
    return \XML_Util::createEndElement($qname);
}
