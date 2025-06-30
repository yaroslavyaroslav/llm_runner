# CreateSearchRequest

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**documents** | Option<**Vec<String>**> | Up to 200 documents to search over, provided as a list of strings.  The maximum document length (in tokens) is 2034 minus the number of tokens in the query.  You should specify either `documents` or a `file`, but not both.  | [optional]
**file** | Option<**String**> | The ID of an uploaded file that contains documents to search over.  You should specify either `documents` or a `file`, but not both.  | [optional]
**max_rerank** | Option<**i32**> | The maximum number of documents to be re-ranked and returned by search.  This flag only takes effect when `file` is set.  | [optional][default to 200]
**query** | **String** | Query to search against the documents. | 
**return_metadata** | Option<**bool**> | A special boolean flag for showing metadata. If set to `true`, each document entry in the returned JSON will contain a \"metadata\" field.  This flag only takes effect when `file` is set.  | [optional][default to false]
**user** | Option<**String**> | A unique identifier representing your end-user, which can help OpenAI to monitor and detect abuse. [Learn more](/docs/guides/safety-best-practices/end-user-ids).  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


