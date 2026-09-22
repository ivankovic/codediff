/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
// One entry per solved `defects4j` fixture (see `test::helper::DIFF_DATASETS`). The dataset's
// 996 fixture directories are all present; this list grows as they are mapped in `human_solver`,
// whose `insert_mod_declaration` adds entries here the same way it does for
// `handmade.rs`/`small.rs`/`full.rs`/`stratified.rs`.
#[cfg(test)]
mod java_defects4j_chart_10_standardtooltiptagfragmentgenerator;
#[cfg(test)]
mod java_defects4j_chart_11_shapeutilities;
#[cfg(test)]
mod java_defects4j_chart_12_multiplepieplot;
#[cfg(test)]
mod java_defects4j_chart_13_borderarrangement;
#[cfg(test)]
mod java_defects4j_chart_14_categoryplot;
#[cfg(test)]
mod java_defects4j_chart_14_xyplot;
#[cfg(test)]
mod java_defects4j_chart_15_pieplot;
#[cfg(test)]
mod java_defects4j_chart_16_defaultintervalcategorydataset;
#[cfg(test)]
mod java_defects4j_chart_17_timeseries;
#[cfg(test)]
mod java_defects4j_chart_18_defaultkeyedvalues;
#[cfg(test)]
mod java_defects4j_chart_18_defaultkeyedvalues2d;
#[cfg(test)]
mod java_defects4j_chart_19_categoryplot;
#[cfg(test)]
mod java_defects4j_chart_1_abstractcategoryitemrenderer;
#[cfg(test)]
mod java_defects4j_chart_20_valuemarker;
#[cfg(test)]
mod java_defects4j_chart_21_defaultboxandwhiskercategorydataset;
#[cfg(test)]
mod java_defects4j_chart_22_keyedobjects2d;
#[cfg(test)]
mod java_defects4j_chart_23_minmaxcategoryrenderer;
#[cfg(test)]
mod java_defects4j_chart_24_graypaintscale;
#[cfg(test)]
mod java_defects4j_chart_25_statisticalbarrenderer;
#[cfg(test)]
mod java_defects4j_chart_26_axis;
#[cfg(test)]
mod java_defects4j_chart_2_datasetutilities;
#[cfg(test)]
mod java_defects4j_chart_3_timeseries;
#[cfg(test)]
mod java_defects4j_chart_4_xyplot;
#[cfg(test)]
mod java_defects4j_chart_5_xyseries;
#[cfg(test)]
mod java_defects4j_chart_6_shapelist;
#[cfg(test)]
mod java_defects4j_chart_7_timeperiodvalues;
#[cfg(test)]
mod java_defects4j_chart_8_week;
#[cfg(test)]
mod java_defects4j_chart_9_timeseries;
#[cfg(test)]
mod java_defects4j_cli_10_parser;
#[cfg(test)]
mod java_defects4j_cli_11_helpformatter;
#[cfg(test)]
mod java_defects4j_cli_12_gnuparser;
#[cfg(test)]
mod java_defects4j_cli_13_argumentimpl;
#[cfg(test)]
mod java_defects4j_cli_13_writeablecommandline;
#[cfg(test)]
mod java_defects4j_cli_13_writeablecommandlineimpl;
#[cfg(test)]
mod java_defects4j_cli_14_groupimpl;
#[cfg(test)]
mod java_defects4j_cli_15_writeablecommandlineimpl;
#[cfg(test)]
mod java_defects4j_cli_16_groupimpl;
#[cfg(test)]
mod java_defects4j_cli_16_option;
#[cfg(test)]
mod java_defects4j_cli_16_optionimpl;
#[cfg(test)]
mod java_defects4j_cli_16_writeablecommandlineimpl;
#[cfg(test)]
mod java_defects4j_cli_17_posixparser;
#[cfg(test)]
mod java_defects4j_cli_18_posixparser;
#[cfg(test)]
mod java_defects4j_cli_19_posixparser;
#[cfg(test)]
mod java_defects4j_cli_1_commandline;
#[cfg(test)]
mod java_defects4j_cli_20_posixparser;
#[cfg(test)]
mod java_defects4j_cli_21_groupimpl;
#[cfg(test)]
mod java_defects4j_cli_21_writeablecommandline;
#[cfg(test)]
mod java_defects4j_cli_21_writeablecommandlineimpl;
#[cfg(test)]
mod java_defects4j_cli_22_posixparser;
#[cfg(test)]
mod java_defects4j_cli_23_helpformatter;
#[cfg(test)]
mod java_defects4j_cli_24_helpformatter;
#[cfg(test)]
mod java_defects4j_cli_25_helpformatter;
#[cfg(test)]
mod java_defects4j_cli_26_optionbuilder;
#[cfg(test)]
mod java_defects4j_cli_27_optiongroup;
#[cfg(test)]
mod java_defects4j_cli_28_parser;
#[cfg(test)]
mod java_defects4j_cli_29_util;
#[cfg(test)]
mod java_defects4j_cli_2_posixparser;
#[cfg(test)]
mod java_defects4j_cli_31_helpformatter;
#[cfg(test)]
mod java_defects4j_cli_31_option;
#[cfg(test)]
mod java_defects4j_cli_31_optionbuilder;
#[cfg(test)]
mod java_defects4j_cli_34_option;
#[cfg(test)]
mod java_defects4j_cli_34_optionbuilder;
#[cfg(test)]
mod java_defects4j_cli_35_options;
#[cfg(test)]
mod java_defects4j_cli_3_typehandler;
#[cfg(test)]
mod java_defects4j_cli_40_typehandler;
#[cfg(test)]
mod java_defects4j_cli_5_util;
#[cfg(test)]
mod java_defects4j_cli_8_helpformatter;
#[cfg(test)]
mod java_defects4j_closure_102_normalize;
#[cfg(test)]
mod java_defects4j_closure_103_controlflowanalysis;
#[cfg(test)]
mod java_defects4j_closure_104_uniontype;
#[cfg(test)]
mod java_defects4j_closure_106_jsdocinfobuilder;
#[cfg(test)]
mod java_defects4j_closure_107_commandlinerunner;
#[cfg(test)]
mod java_defects4j_closure_10_nodeutil;
#[cfg(test)]
mod java_defects4j_closure_113_processclosureprimitives;
#[cfg(test)]
mod java_defects4j_closure_114_nameanalyzer;
#[cfg(test)]
mod java_defects4j_closure_119_globalnamespace;
#[cfg(test)]
mod java_defects4j_closure_11_typecheck;
#[cfg(test)]
mod java_defects4j_closure_123_codegenerator;
#[cfg(test)]
mod java_defects4j_closure_124_exploitassigns;
#[cfg(test)]
mod java_defects4j_closure_125_typecheck;
#[cfg(test)]
mod java_defects4j_closure_130_collapseproperties;
#[cfg(test)]
mod java_defects4j_closure_133_jsdocinfoparser;
#[cfg(test)]
mod java_defects4j_closure_135_devirtualizeprototypemethods;
#[cfg(test)]
mod java_defects4j_closure_137_normalize;
#[cfg(test)]
mod java_defects4j_closure_138_closurereverseabstractinterpreter;
#[cfg(test)]
mod java_defects4j_closure_13_peepholeoptimizationspass;
#[cfg(test)]
mod java_defects4j_closure_144_functiontype;
#[cfg(test)]
mod java_defects4j_closure_147_checkglobalthis;
#[cfg(test)]
mod java_defects4j_closure_149_commandlinerunner;
#[cfg(test)]
mod java_defects4j_closure_14_controlflowanalysis;
#[cfg(test)]
mod java_defects4j_closure_165_recordtypebuilder;
#[cfg(test)]
mod java_defects4j_closure_168_typedscopecreator;
#[cfg(test)]
mod java_defects4j_closure_18_compiler;
#[cfg(test)]
mod java_defects4j_closure_19_chainablereverseabstractinterpreter;
#[cfg(test)]
mod java_defects4j_closure_1_removeunusedvars;
#[cfg(test)]
mod java_defects4j_closure_28_inlinecostestimator;
#[cfg(test)]
mod java_defects4j_closure_30_flowsensitiveinlinevariables;
#[cfg(test)]
mod java_defects4j_closure_31_compiler;
#[cfg(test)]
mod java_defects4j_closure_34_codeprinter;
#[cfg(test)]
mod java_defects4j_closure_37_nodetraversal;
#[cfg(test)]
mod java_defects4j_closure_38_codeconsumer;
#[cfg(test)]
mod java_defects4j_closure_44_codeconsumer;
#[cfg(test)]
mod java_defects4j_closure_52_codegenerator;
#[cfg(test)]
mod java_defects4j_closure_57_closurecodingconvention;
#[cfg(test)]
mod java_defects4j_closure_62_lightweightmessageformatter;
#[cfg(test)]
mod java_defects4j_closure_65_codegenerator;
#[cfg(test)]
mod java_defects4j_closure_66_typecheck;
#[cfg(test)]
mod java_defects4j_closure_67_analyzeprototypeproperties;
#[cfg(test)]
mod java_defects4j_closure_70_typedscopecreator;
#[cfg(test)]
mod java_defects4j_closure_71_checkaccesscontrols;
#[cfg(test)]
mod java_defects4j_closure_72_functiontoblockmutator;
#[cfg(test)]
mod java_defects4j_closure_72_renamelabels;
#[cfg(test)]
mod java_defects4j_closure_73_codegenerator;
#[cfg(test)]
mod java_defects4j_closure_77_codegenerator;
#[cfg(test)]
mod java_defects4j_closure_78_peepholefoldconstants;
#[cfg(test)]
mod java_defects4j_closure_79_normalize;
#[cfg(test)]
mod java_defects4j_closure_79_varcheck;
#[cfg(test)]
mod java_defects4j_closure_80_nodeutil;
#[cfg(test)]
mod java_defects4j_closure_86_nodeutil;
#[cfg(test)]
mod java_defects4j_closure_89_globalnamespace;
#[cfg(test)]
mod java_defects4j_closure_90_functiontypebuilder;
#[cfg(test)]
mod java_defects4j_closure_92_processclosureprimitives;
#[cfg(test)]
mod java_defects4j_codec_10_caverphone;
#[cfg(test)]
mod java_defects4j_codec_16_base32;
#[cfg(test)]
mod java_defects4j_codec_17_stringutils;
#[cfg(test)]
mod java_defects4j_codec_1_caverphone;
#[cfg(test)]
mod java_defects4j_codec_1_metaphone;
#[cfg(test)]
mod java_defects4j_codec_1_soundexutils;
#[cfg(test)]
mod java_defects4j_codec_2_base64;
#[cfg(test)]
mod java_defects4j_codec_3_doublemetaphone;
#[cfg(test)]
mod java_defects4j_codec_4_base64;
#[cfg(test)]
mod java_defects4j_codec_7_base64;
#[cfg(test)]
mod java_defects4j_codec_8_base64inputstream;
#[cfg(test)]
mod java_defects4j_codec_9_base64;
#[cfg(test)]
mod java_defects4j_collections_26_multikey;
#[cfg(test)]
mod java_defects4j_compress_11_archivestreamfactory;
#[cfg(test)]
mod java_defects4j_compress_19_zip64extendedinformationextrafield;
#[cfg(test)]
mod java_defects4j_compress_1_cpioarchiveoutputstream;
#[cfg(test)]
mod java_defects4j_compress_23_coders;
#[cfg(test)]
mod java_defects4j_compress_25_ziparchiveinputstream;
#[cfg(test)]
mod java_defects4j_compress_26_ioutils;
#[cfg(test)]
mod java_defects4j_compress_29_cpioarchiveinputstream;
#[cfg(test)]
mod java_defects4j_compress_29_cpioarchiveoutputstream;
#[cfg(test)]
mod java_defects4j_compress_29_dumparchiveinputstream;
#[cfg(test)]
mod java_defects4j_compress_29_tararchiveinputstream;
#[cfg(test)]
mod java_defects4j_compress_29_tararchiveoutputstream;
#[cfg(test)]
mod java_defects4j_compress_29_ziparchiveinputstream;
#[cfg(test)]
mod java_defects4j_compress_33_deflatecompressorinputstream;
#[cfg(test)]
mod java_defects4j_compress_38_tararchiveentry;
#[cfg(test)]
mod java_defects4j_compress_42_unixstat;
#[cfg(test)]
mod java_defects4j_compress_42_ziparchiveentry;
#[cfg(test)]
mod java_defects4j_compress_44_checksumcalculatinginputstream;
#[cfg(test)]
mod java_defects4j_compress_4_changesetperformer;
#[cfg(test)]
mod java_defects4j_compress_4_cpioarchiveoutputstream;
#[cfg(test)]
mod java_defects4j_compress_4_tararchiveoutputstream;
#[cfg(test)]
mod java_defects4j_compress_4_ziparchiveoutputstream;
#[cfg(test)]
mod java_defects4j_csv_1_extendedbufferedreader;
#[cfg(test)]
mod java_defects4j_csv_2_csvrecord;
#[cfg(test)]
mod java_defects4j_gson_11_typeadapters;
#[cfg(test)]
mod java_defects4j_gson_5_iso8601utils;
#[cfg(test)]
mod java_defects4j_gson_6_jsonadapterannotationtypeadapterfactory;
#[cfg(test)]
mod java_defects4j_jacksoncore_11_bytequadscanonicalizer;
#[cfg(test)]
mod java_defects4j_jacksoncore_13_jsongeneratorimpl;
#[cfg(test)]
mod java_defects4j_jacksoncore_26_nonblockingjsonparser;
#[cfg(test)]
mod java_defects4j_jacksondatabind_105_jdkdeserializers;
#[cfg(test)]
mod java_defects4j_jacksondatabind_111_atomicreferencedeserializer;
#[cfg(test)]
mod java_defects4j_jacksondatabind_15_javatype;
#[cfg(test)]
mod java_defects4j_jacksondatabind_16_annotationmap;
#[cfg(test)]
mod java_defects4j_jacksondatabind_1_beanpropertywriter;
#[cfg(test)]
mod java_defects4j_jacksondatabind_25_simpleabstracttyperesolver;
#[cfg(test)]
mod java_defects4j_jacksondatabind_34_numberserializer;
#[cfg(test)]
mod java_defects4j_jacksondatabind_39_nullifyingdeserializer;
#[cfg(test)]
mod java_defects4j_jacksondatabind_49_writableobjectid;
#[cfg(test)]
mod java_defects4j_jacksondatabind_59_typefactory;
#[cfg(test)]
mod java_defects4j_jacksondatabind_79_objectidinfo;
#[cfg(test)]
mod java_defects4j_jacksondatabind_94_subtypevalidator;
#[cfg(test)]
mod java_defects4j_jacksondatabind_99_referencetype;
#[cfg(test)]
mod java_defects4j_jsoup_16_documenttype;
#[cfg(test)]
mod java_defects4j_jsoup_17_treebuilderstate;
#[cfg(test)]
mod java_defects4j_jsoup_24_tokeniserstate;
#[cfg(test)]
mod java_defects4j_jsoup_26_cleaner;
#[cfg(test)]
mod java_defects4j_jsoup_2_parser;
#[cfg(test)]
mod java_defects4j_jsoup_31_token;
#[cfg(test)]
mod java_defects4j_jsoup_31_tokeniserstate;
#[cfg(test)]
mod java_defects4j_jsoup_33_htmltreebuilder;
#[cfg(test)]
mod java_defects4j_jsoup_35_htmltreebuilderstate;
#[cfg(test)]
mod java_defects4j_jsoup_39_datautil;
#[cfg(test)]
mod java_defects4j_jsoup_40_documenttype;
#[cfg(test)]
mod java_defects4j_jsoup_52_xmldeclaration;
#[cfg(test)]
mod java_defects4j_jsoup_54_w3cdom;
#[cfg(test)]
mod java_defects4j_jsoup_55_tokeniserstate;
#[cfg(test)]
mod java_defects4j_jsoup_56_documenttype;
#[cfg(test)]
mod java_defects4j_jsoup_76_htmltreebuilderstate;
#[cfg(test)]
mod java_defects4j_jsoup_79_leafnode;
#[cfg(test)]
mod java_defects4j_jsoup_86_comment;
#[cfg(test)]
mod java_defects4j_jsoup_91_uncheckedioexception;
#[cfg(test)]
mod java_defects4j_jsoup_92_parsesettings;
#[cfg(test)]
mod java_defects4j_jsoup_92_xmltreebuilder;
#[cfg(test)]
mod java_defects4j_jsoup_93_formelement;
#[cfg(test)]
mod java_defects4j_jxpath_15_unioncontext;
#[cfg(test)]
mod java_defects4j_jxpath_18_attributecontext;
#[cfg(test)]
mod java_defects4j_jxpath_7_coreoperationgreaterthan;
#[cfg(test)]
mod java_defects4j_jxpath_7_coreoperationgreaterthanorequal;
#[cfg(test)]
mod java_defects4j_jxpath_7_coreoperationlessthan;
#[cfg(test)]
mod java_defects4j_jxpath_7_coreoperationlessthanorequal;
#[cfg(test)]
mod java_defects4j_jxpath_7_coreoperationrelationalexpression;
#[cfg(test)]
mod java_defects4j_lang_14_stringutils;
#[cfg(test)]
mod java_defects4j_lang_17_charsequencetranslator;
#[cfg(test)]
mod java_defects4j_lang_19_numericentityunescaper;
#[cfg(test)]
mod java_defects4j_lang_28_numericentityunescaper;
#[cfg(test)]
mod java_defects4j_lang_38_fastdateformat;
#[cfg(test)]
mod java_defects4j_lang_43_extendedmessageformat;
#[cfg(test)]
mod java_defects4j_lang_4_lookuptranslator;
#[cfg(test)]
mod java_defects4j_lang_51_booleanutils;
#[cfg(test)]
mod java_defects4j_lang_54_localeutils;
#[cfg(test)]
mod java_defects4j_lang_64_valuedenum;
#[cfg(test)]
mod java_defects4j_lang_6_charsequencetranslator;
#[cfg(test)]
mod java_defects4j_math_103_normaldistributionimpl;
#[cfg(test)]
mod java_defects4j_math_10_dscompiler;
#[cfg(test)]
mod java_defects4j_math_14_weight;
#[cfg(test)]
mod java_defects4j_math_22_fdistribution;
#[cfg(test)]
mod java_defects4j_math_22_uniformrealdistribution;
#[cfg(test)]
mod java_defects4j_math_35_elitisticlistpopulation;
#[cfg(test)]
mod java_defects4j_math_6_baseoptimizer;
#[cfg(test)]
mod java_defects4j_math_6_cmaesoptimizer;
#[cfg(test)]
mod java_defects4j_math_70_bisectionsolver;
#[cfg(test)]
mod java_defects4j_math_95_fdistributionimpl;
#[cfg(test)]
mod java_defects4j_mockito_11_delegatingmethod;
#[cfg(test)]
mod java_defects4j_mockito_12_genericmaster;
#[cfg(test)]
mod java_defects4j_mockito_15_finalmockcandidatefilter;
#[cfg(test)]
mod java_defects4j_mockito_17_mocksettingsimpl;
#[cfg(test)]
mod java_defects4j_mockito_19_finalmockcandidatefilter;
#[cfg(test)]
mod java_defects4j_mockito_19_mockcandidatefilter;
#[cfg(test)]
mod java_defects4j_mockito_19_namebasedcandidatefilter;
#[cfg(test)]
mod java_defects4j_mockito_19_typebasedcandidatefilter;
#[cfg(test)]
mod java_defects4j_mockito_21_constructorinstantiator;
#[cfg(test)]
mod java_defects4j_mockito_22_equality;
#[cfg(test)]
mod java_defects4j_mockito_29_same;
#[cfg(test)]
mod java_defects4j_mockito_2_timer;
#[cfg(test)]
mod java_defects4j_mockito_30_returnssmartnulls;
#[cfg(test)]
mod java_defects4j_mockito_31_returnssmartnulls;
#[cfg(test)]
mod java_defects4j_mockito_32_spyannotationengine;
#[cfg(test)]
mod java_defects4j_mockito_37_answersvalidator;
#[cfg(test)]
mod java_defects4j_mockito_38_argumentmatchingtool;
#[cfg(test)]
mod java_defects4j_mockito_5_verificationovertimeimpl;
#[cfg(test)]
mod java_defects4j_mockito_7_genericmetadatasupport;
#[cfg(test)]
mod java_defects4j_mockito_9_callsrealmethods;
